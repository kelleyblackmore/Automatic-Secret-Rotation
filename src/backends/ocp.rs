#![cfg(feature = "ocp")]

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use async_trait::async_trait;
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::ByteString;
use kube::{
    api::{ListParams, ObjectMeta, Patch, PatchParams, PostParams},
    Api, Client,
};

use super::secret_backend::{SecretBackend, SecretData};
use crate::config::OcpConfig;

// Annotation prefix for ASR rotation metadata
const ANNOTATION_PREFIX: &str = "asr.io/";

pub struct OcpBackend {
    client: Client,
    namespace: String,
}

impl OcpBackend {
    pub async fn new(config: &OcpConfig) -> Result<Self> {
        let client = if let Some(ref kubeconfig_path) = config.kubeconfig {
            let kubeconfig = kube::config::Kubeconfig::read_from(kubeconfig_path)
                .with_context(|| format!("Failed to read kubeconfig from {}", kubeconfig_path))?;

            let opts = kube::config::KubeConfigOptions {
                context: config.context.clone(),
                ..Default::default()
            };

            let config = kube::Config::from_custom_kubeconfig(kubeconfig, &opts)
                .await
                .context("Failed to build Kubernetes config from kubeconfig")?;

            Client::try_from(config).context("Failed to create Kubernetes client")?
        } else {
            // Use in-cluster auth or default kubeconfig (~/.kube/config)
            Client::try_default()
                .await
                .context("Failed to create Kubernetes client (tried in-cluster auth and ~/.kube/config)")?
        };

        Ok(Self {
            client,
            namespace: config.namespace.clone(),
        })
    }

    /// Convert ASR path to Kubernetes secret name (/ → -)
    fn path_to_name(path: &str) -> String {
        path.replace('/', "-")
    }

    /// Convert Kubernetes secret name to ASR path
    fn name_to_path(name: &str) -> String {
        name.replace('-', "/")
    }

    fn annotation_key(meta_key: &str) -> String {
        format!(
            "{}{}",
            ANNOTATION_PREFIX,
            meta_key.replace('_', "-")
        )
    }

    fn meta_key_from_annotation(annotation: &str) -> Option<String> {
        annotation
            .strip_prefix(ANNOTATION_PREFIX)
            .map(|k| k.replace('-', "_"))
    }

    fn secrets_api(&self) -> Api<Secret> {
        Api::namespaced(self.client.clone(), &self.namespace)
    }
}

#[async_trait]
impl SecretBackend for OcpBackend {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        let name = Self::path_to_name(path);
        let api = self.secrets_api();

        let secret = api
            .get(&name)
            .await
            .with_context(|| format!("Failed to read Kubernetes secret: {}", path))?;

        let data: HashMap<String, String> = secret
            .data
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| {
                String::from_utf8(v.0).ok().map(|s| (k, s))
            })
            .collect();

        let metadata: Option<HashMap<String, String>> = secret
            .metadata
            .annotations
            .map(|annotations| {
                annotations
                    .into_iter()
                    .filter_map(|(k, v)| {
                        Self::meta_key_from_annotation(&k).map(|mk| (mk, v))
                    })
                    .collect()
            });

        Ok(SecretData { data, metadata })
    }

    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()> {
        let name = Self::path_to_name(path);
        let api = self.secrets_api();

        let secret_data: BTreeMap<String, ByteString> = data
            .into_iter()
            .map(|(k, v)| (k, ByteString(v.into_bytes())))
            .collect();

        let secret = Secret {
            metadata: ObjectMeta {
                name: Some(name.clone()),
                namespace: Some(self.namespace.clone()),
                ..Default::default()
            },
            data: Some(secret_data),
            ..Default::default()
        };

        // Try create first, then update if it already exists
        match api.create(&PostParams::default(), &secret).await {
            Ok(_) => {}
            Err(kube::Error::Api(err)) if err.code == 409 => {
                // Secret already exists — patch it
                let patch = serde_json::json!({
                    "data": secret.data
                });
                api.patch(
                    &name,
                    &PatchParams::apply("asr"),
                    &Patch::Merge(&patch),
                )
                .await
                .with_context(|| format!("Failed to update Kubernetes secret: {}", path))?;
            }
            Err(e) => {
                return Err(e).with_context(|| format!("Failed to create Kubernetes secret: {}", path));
            }
        }

        Ok(())
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()> {
        let name = Self::path_to_name(path);
        let api = self.secrets_api();

        let annotations: BTreeMap<String, String> = metadata
            .into_iter()
            .map(|(k, v)| (Self::annotation_key(&k), v))
            .collect();

        let patch = serde_json::json!({
            "metadata": {
                "annotations": annotations
            }
        });

        api.patch(
            &name,
            &PatchParams::apply("asr"),
            &Patch::Merge(&patch),
        )
        .await
        .with_context(|| format!("Failed to update Kubernetes secret annotations: {}", path))?;

        Ok(())
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        let name = Self::path_to_name(path);
        let api = self.secrets_api();

        let secret = api
            .get(&name)
            .await
            .with_context(|| format!("Failed to read Kubernetes secret: {}", path))?;

        let metadata: HashMap<String, String> = secret
            .metadata
            .annotations
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| {
                Self::meta_key_from_annotation(&k).map(|mk| (mk, v))
            })
            .collect();

        Ok(metadata)
    }

    async fn list_secrets(&self, path: &str) -> Result<Vec<String>> {
        let api = self.secrets_api();
        let lp = ListParams::default();

        let secret_list = api
            .list(&lp)
            .await
            .context("Failed to list Kubernetes secrets")?;

        let prefix = if path.is_empty() {
            String::new()
        } else {
            Self::path_to_name(path)
        };

        let names: Vec<String> = secret_list
            .items
            .into_iter()
            .filter_map(|s| s.metadata.name)
            .filter(|n| {
                // Only list secrets managed by ASR (have at least one ASR annotation)
                // Skip service account tokens and other system secrets
                prefix.is_empty() || n.starts_with(&prefix)
            })
            .map(|n| Self::name_to_path(&n))
            .collect();

        Ok(names)
    }

    fn backend_type(&self) -> &'static str {
        "OpenShift/Kubernetes"
    }
}
