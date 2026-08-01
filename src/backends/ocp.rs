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
            Client::try_default().await.context(
                "Failed to create Kubernetes client (tried in-cluster auth and ~/.kube/config)",
            )?
        };

        Ok(Self {
            client,
            namespace: config.namespace.clone(),
        })
    }

    /// Convert ASR path to Kubernetes secret name (/ → -)
    fn path_to_name(path: &str) -> String {
        crate::util::path::path_to_k8s_name(path)
    }

    /// Convert Kubernetes secret name to ASR path
    fn name_to_path(name: &str) -> String {
        crate::util::path::k8s_name_to_path(name)
    }

    fn annotation_key(meta_key: &str) -> String {
        format!("{}{}", ANNOTATION_PREFIX, meta_key.replace('_', "-"))
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
            .filter_map(|(k, v)| String::from_utf8(v.0).ok().map(|s| (k, s)))
            .collect();

        let metadata: Option<HashMap<String, String>> =
            secret.metadata.annotations.map(|annotations| {
                annotations
                    .into_iter()
                    .filter_map(|(k, v)| Self::meta_key_from_annotation(&k).map(|mk| (mk, v)))
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
                api.patch(&name, &PatchParams::apply("asr"), &Patch::Merge(&patch))
                    .await
                    .with_context(|| format!("Failed to update Kubernetes secret: {}", path))?;
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Failed to create Kubernetes secret: {}", path));
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

        api.patch(&name, &PatchParams::apply("asr"), &Patch::Merge(&patch))
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
            .filter_map(|(k, v)| Self::meta_key_from_annotation(&k).map(|mk| (mk, v)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Request, Response};
    use http_body_util::BodyExt;
    use kube::client::Body;

    /// A request the backend sent to the (mocked) API server.
    #[derive(Clone, Debug)]
    struct Recorded {
        method: String,
        uri: String,
        body: String,
    }

    #[derive(Clone, Default)]
    struct RequestLog(std::sync::Arc<std::sync::Mutex<Vec<Recorded>>>);

    impl RequestLog {
        fn calls(&self) -> Vec<Recorded> {
            self.0.lock().unwrap().clone()
        }
    }

    /// Build a `Client` backed by canned responses instead of a real API
    /// server, recording each request so assertions can check what was sent.
    ///
    /// `kube::Client::new` accepts any tower `Service`, which is what makes it
    /// possible to cover this module's request/response mapping without a
    /// cluster. The `ocp` backend previously had no tests at all, so the
    /// kube 0.98 -> 4.x bump could have changed serialization with nothing to
    /// catch it.
    ///
    /// Responses are served in order; the last one repeats if the code makes
    /// more calls than were queued.
    fn mock_client(responses: Vec<(u16, serde_json::Value)>) -> (Client, RequestLog) {
        let log = RequestLog::default();
        let sink = log.clone();
        let queue = std::sync::Arc::new(std::sync::Mutex::new(responses));

        let service = tower::service_fn(move |req: Request<Body>| {
            let sink = sink.clone();
            let queue = queue.clone();
            async move {
                let method = req.method().to_string();
                let uri = req.uri().to_string();
                let bytes = req.into_body().collect().await.unwrap().to_bytes();
                sink.0.lock().unwrap().push(Recorded {
                    method,
                    uri,
                    body: String::from_utf8_lossy(&bytes).into_owned(),
                });

                let (status, body) = {
                    let mut q = queue.lock().unwrap();
                    if q.len() > 1 {
                        q.remove(0)
                    } else {
                        q[0].clone()
                    }
                };

                Ok::<_, std::convert::Infallible>(
                    Response::builder()
                        .status(status)
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string().into_bytes()))
                        .unwrap(),
                )
            }
        });
        (Client::new(service, "default"), log)
    }

    fn ok(body: serde_json::Value) -> Vec<(u16, serde_json::Value)> {
        vec![(200, body)]
    }

    fn backend(client: Client) -> OcpBackend {
        OcpBackend {
            client,
            namespace: "asr-test".to_string(),
        }
    }

    fn secret_json(annotations: serde_json::Value, data: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": { "name": "myapp-db", "namespace": "asr-test", "annotations": annotations },
            "data": data
        })
    }

    fn failure(code: u16, reason: &str, message: &str) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "v1",
            "kind": "Status",
            "status": "Failure",
            "message": message,
            "reason": reason,
            "code": code
        })
    }

    fn one_entry(key: &str, value: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(key.to_string(), value.to_string());
        m
    }

    // ---- pure helpers -----------------------------------------------------

    #[test]
    fn annotation_key_round_trips_through_underscores() {
        // Kubernetes annotations conventionally use '-', ASR metadata uses '_'.
        let key = OcpBackend::annotation_key("rotation_period_months");
        assert_eq!(key, "asr.io/rotation-period-months");
        assert_eq!(
            OcpBackend::meta_key_from_annotation(&key).as_deref(),
            Some("rotation_period_months")
        );
    }

    #[test]
    fn meta_key_ignores_foreign_annotations() {
        assert_eq!(
            OcpBackend::meta_key_from_annotation("kubectl.kubernetes.io/last-applied"),
            None
        );
    }

    #[test]
    fn path_and_name_convert_both_ways() {
        assert_eq!(OcpBackend::path_to_name("myapp/db"), "myapp-db");
        assert_eq!(OcpBackend::name_to_path("myapp-db"), "myapp/db");
    }

    // ---- read path --------------------------------------------------------

    #[tokio::test]
    async fn read_secret_decodes_data_and_strips_annotation_prefix() {
        let (client, log) = mock_client(ok(secret_json(
            serde_json::json!({
                "asr.io/rotation-enabled": "true",
                "kubectl.kubernetes.io/last-applied": "{}"
            }),
            // base64("s3cret")
            serde_json::json!({ "password": "czNjcmV0" }),
        )));

        let secret = backend(client).read_secret("myapp/db").await.unwrap();

        assert_eq!(
            secret.data.get("password").map(String::as_str),
            Some("s3cret")
        );

        let meta = secret.metadata.expect("annotations should map to metadata");
        assert_eq!(
            meta.get("rotation_enabled").map(String::as_str),
            Some("true")
        );
        // Non-ASR annotations must not leak into rotation metadata.
        assert!(!meta.contains_key("last_applied"));

        let call = &log.calls()[0];
        assert_eq!(call.method, "GET");
        assert!(
            call.uri
                .contains("/api/v1/namespaces/asr-test/secrets/myapp-db"),
            "unexpected request URI: {}",
            call.uri
        );
    }

    #[tokio::test]
    async fn read_metadata_returns_only_asr_annotations() {
        let (client, _) = mock_client(ok(secret_json(
            serde_json::json!({
                "asr.io/last-rotated": "2026-08-01T00:00:00Z",
                "other.io/thing": "ignored"
            }),
            serde_json::json!({}),
        )));

        let meta = backend(client).read_metadata("myapp/db").await.unwrap();

        assert_eq!(meta.len(), 1);
        assert_eq!(
            meta.get("last_rotated").map(String::as_str),
            Some("2026-08-01T00:00:00Z")
        );
    }

    #[tokio::test]
    async fn list_secrets_filters_by_prefix_and_maps_names_back_to_paths() {
        let (client, _) = mock_client(ok(serde_json::json!({
            "apiVersion": "v1",
            "kind": "SecretList",
            "metadata": {},
            "items": [
                { "metadata": { "name": "myapp-db" } },
                { "metadata": { "name": "myapp-api" } },
                { "metadata": { "name": "otherapp-db" } }
            ]
        })));

        let mut names = backend(client).list_secrets("myapp").await.unwrap();
        names.sort();

        assert_eq!(names, vec!["myapp/api".to_string(), "myapp/db".to_string()]);
    }

    #[tokio::test]
    async fn read_secret_surfaces_api_errors_with_the_asr_path() {
        let (client, _) = mock_client(vec![(
            404,
            failure(404, "NotFound", "secrets myapp-db not found"),
        )]);

        let err = backend(client)
            .read_secret("myapp/db")
            .await
            .expect_err("404 should be an error");

        assert!(
            err.to_string().contains("myapp/db"),
            "error should name the ASR path, got: {err}"
        );
    }

    // ---- write path -------------------------------------------------------

    #[tokio::test]
    async fn write_secret_creates_when_absent() {
        let (client, log) = mock_client(ok(secret_json(
            serde_json::json!({}),
            serde_json::json!({}),
        )));

        backend(client)
            .write_secret("myapp/db", one_entry("password", "s3cret"))
            .await
            .unwrap();

        let calls = log.calls();
        assert_eq!(calls.len(), 1, "a successful create should not also patch");
        assert_eq!(calls[0].method, "POST");
        // Secret data goes over the wire base64-encoded.
        assert!(
            calls[0].body.contains("czNjcmV0"),
            "create body should carry base64 data, got: {}",
            calls[0].body
        );
    }

    /// The 409 fallback is the subtlest branch in this module: `create` is
    /// tried first, and only a conflict may fall through to a merge patch.
    #[tokio::test]
    async fn write_secret_falls_back_to_patch_on_conflict() {
        let (client, log) = mock_client(vec![
            (
                409,
                failure(409, "AlreadyExists", "secrets myapp-db already exists"),
            ),
            (
                200,
                secret_json(serde_json::json!({}), serde_json::json!({})),
            ),
        ]);

        backend(client)
            .write_secret("myapp/db", one_entry("password", "s3cret"))
            .await
            .unwrap();

        let calls = log.calls();
        assert_eq!(calls.len(), 2, "conflict should trigger exactly one patch");
        assert_eq!(calls[0].method, "POST");
        assert_eq!(calls[1].method, "PATCH");
        assert!(
            calls[1].body.contains("czNjcmV0"),
            "patch body should carry the new data, got: {}",
            calls[1].body
        );
    }

    /// A non-409 failure must propagate, not be silently patched over.
    #[tokio::test]
    async fn write_secret_propagates_non_conflict_errors() {
        let (client, log) = mock_client(vec![(
            403,
            failure(403, "Forbidden", "secrets is forbidden"),
        )]);

        let err = backend(client)
            .write_secret("myapp/db", one_entry("password", "s3cret"))
            .await
            .expect_err("403 should be an error");

        assert_eq!(log.calls().len(), 1, "403 must not fall through to a patch");
        assert!(
            err.to_string().contains("myapp/db"),
            "error should name the ASR path, got: {err}"
        );
    }

    #[tokio::test]
    async fn update_metadata_writes_prefixed_annotations() {
        let (client, log) = mock_client(ok(secret_json(
            serde_json::json!({}),
            serde_json::json!({}),
        )));

        backend(client)
            .update_metadata("myapp/db", one_entry("rotation_enabled", "true"))
            .await
            .unwrap();

        let calls = log.calls();
        assert_eq!(calls[0].method, "PATCH");
        assert!(
            calls[0].body.contains("asr.io/rotation-enabled"),
            "underscores should become dashes under the ASR prefix, got: {}",
            calls[0].body
        );
    }
}
