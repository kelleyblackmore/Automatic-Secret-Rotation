#![cfg(feature = "azure")]

//! Azure Key Vault backend via REST API.
//!
//! Authentication: set `AZURE_ACCESS_TOKEN` to a valid bearer token, or run on
//! an Azure resource with a managed identity (the token is fetched from IMDS).
//!
//! Required env vars:
//!   AZURE_VAULT_URL  – e.g. "https://my-vault.vault.azure.net"
//!   AZURE_ACCESS_TOKEN (optional) – pre-fetched token; otherwise IMDS is used

use std::collections::HashMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::secret_backend::{SecretBackend, SecretData};
use crate::config::AzureConfig;

const API_VERSION: &str = "7.4";
const META_TAG_PREFIX: &str = "asr_";

pub struct AzureKeyVaultBackend {
    vault_url: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct KvSecretBundle {
    value: Option<String>,
    tags: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct KvSecretList {
    value: Vec<KvSecretListItem>,
    #[serde(rename = "nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct KvSecretListItem {
    id: String,
}

impl AzureKeyVaultBackend {
    pub async fn new(config: &AzureConfig) -> Result<Self> {
        crate::util::http::require_secure_url(
            &config.vault_url,
            "Azure Key Vault URL (carries the access token)",
        )?;
        Ok(Self {
            vault_url: config.vault_url.trim_end_matches('/').to_string(),
            client: crate::util::http::build_http_client(30)?,
        })
    }

    async fn bearer_token(&self) -> Result<String> {
        // Prefer an explicit env-var token (useful in CI)
        if let Ok(token) = std::env::var("AZURE_ACCESS_TOKEN") {
            return Ok(token);
        }

        // Fall back to Azure IMDS (works on Azure VMs / App Service / AKS)
        let resp = self
            .client
            .get("http://169.254.169.254/metadata/identity/oauth2/token")
            .query(&[
                ("api-version", "2018-02-01"),
                ("resource", "https://vault.azure.net"),
            ])
            .header("Metadata", "true")
            .send()
            .await
            .context("Failed to fetch token from Azure IMDS")?;

        #[derive(Deserialize)]
        struct ImdsToken {
            access_token: String,
        }
        let token: ImdsToken = resp
            .json()
            .await
            .context("Failed to parse Azure IMDS token response")?;

        Ok(token.access_token)
    }

    fn path_to_name(path: &str) -> String {
        crate::util::path::path_to_k8s_name(path)
    }

    fn name_to_path(name: &str) -> String {
        crate::util::path::k8s_name_to_path(name)
    }

    async fn get_secret_bundle(&self, name: &str) -> Result<KvSecretBundle> {
        let token = self.bearer_token().await?;
        let url = format!(
            "{}/secrets/{}?api-version={}",
            self.vault_url, name, API_VERSION
        );
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .with_context(|| format!("Failed to GET secret: {}", name))?;

        crate::util::http::require_success(resp, &format!("Azure Key Vault GET {}", name))
            .await?
            .json::<KvSecretBundle>()
            .await
            .context("Failed to parse Azure Key Vault response")
    }
}

#[async_trait]
impl SecretBackend for AzureKeyVaultBackend {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        let name = Self::path_to_name(path);
        let bundle = self.get_secret_bundle(&name).await?;

        let value = bundle.value.unwrap_or_default();
        let data: HashMap<String, String> =
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&value) {
                map
            } else {
                let mut m = HashMap::new();
                m.insert("value".to_string(), value);
                m
            };

        let metadata = bundle.tags.map(|tags| {
            tags.into_iter()
                .filter_map(|(k, v)| {
                    k.strip_prefix(META_TAG_PREFIX)
                        .map(|stripped| (stripped.to_string(), v))
                })
                .collect()
        });

        Ok(SecretData { data, metadata })
    }

    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()> {
        let name = Self::path_to_name(path);
        let token = self.bearer_token().await?;
        let value = serde_json::to_string(&data).context("Failed to serialize secret data")?;

        let url = format!(
            "{}/secrets/{}?api-version={}",
            self.vault_url, name, API_VERSION
        );
        let body = serde_json::json!({ "value": value });

        let resp = self
            .client
            .put(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to PUT secret: {}", path))?;

        crate::util::http::require_success(resp, &format!("Azure Key Vault PUT {}", path)).await?;

        Ok(())
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()> {
        let name = Self::path_to_name(path);
        let token = self.bearer_token().await?;

        let tags: HashMap<String, String> = metadata
            .into_iter()
            .map(|(k, v)| (format!("{}{}", META_TAG_PREFIX, k), v))
            .collect();

        let url = format!(
            "{}/secrets/{}?api-version={}",
            self.vault_url, name, API_VERSION
        );
        let body = serde_json::json!({ "tags": tags });

        let resp = self
            .client
            .patch(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to PATCH secret metadata: {}", path))?;

        crate::util::http::require_success(resp, &format!("Azure Key Vault PATCH {}", path))
            .await?;

        Ok(())
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        let name = Self::path_to_name(path);
        let bundle = self.get_secret_bundle(&name).await?;
        let metadata = bundle
            .tags
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| {
                k.strip_prefix(META_TAG_PREFIX)
                    .map(|stripped| (stripped.to_string(), v))
            })
            .collect();
        Ok(metadata)
    }

    async fn list_secrets(&self, path: &str) -> Result<Vec<String>> {
        let token = self.bearer_token().await?;
        let prefix = if path.is_empty() {
            String::new()
        } else {
            Self::path_to_name(path)
        };

        let mut all = Vec::new();
        let mut url = format!(
            "{}/secrets?api-version={}&maxresults=25",
            self.vault_url, API_VERSION
        );

        loop {
            let resp = self
                .client
                .get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .context("Failed to list Azure Key Vault secrets")?;

            let list: KvSecretList =
                crate::util::http::require_success(resp, "Azure Key Vault LIST")
                    .await?
                    .json()
                    .await
                    .context("Failed to parse secret list")?;

            for item in list.value {
                // id is like https://vault.vault.azure.net/secrets/my-secret
                if let Some(name) = item.id.rsplit('/').next() {
                    if prefix.is_empty() || name.starts_with(&prefix) {
                        all.push(Self::name_to_path(name));
                    }
                }
            }

            match list.next_link {
                Some(next) => {
                    // The bearer token goes to whatever URL the server names
                    // here, so it must stay on the vault's own origin: no
                    // downgrade to http:// and no other host.
                    let same_origin = matches!(
                        (reqwest::Url::parse(&next), reqwest::Url::parse(&self.vault_url)),
                        (Ok(n), Ok(v)) if n.origin() == v.origin()
                    );
                    if !same_origin {
                        anyhow::bail!(
                            "Azure Key Vault returned a nextLink outside {}; refusing to send \
                             the access token there",
                            self.vault_url
                        );
                    }
                    url = next
                }
                None => break,
            }
        }

        Ok(all)
    }

    fn backend_type(&self) -> &'static str {
        "Azure Key Vault"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn backend_for(server: &mockito::Server) -> AzureKeyVaultBackend {
        // Only this module's tests touch AZURE_ACCESS_TOKEN.
        std::env::set_var("AZURE_ACCESS_TOKEN", "test-token");
        AzureKeyVaultBackend::new(&AzureConfig {
            vault_url: server.url(),
            tenant_id: None,
        })
        .await
        .unwrap()
    }

    fn page(names: &[&str], next: Option<String>) -> String {
        serde_json::json!({
            "value": names.iter().map(|n| serde_json::json!({"id": format!("https://v/secrets/{n}")})).collect::<Vec<_>>(),
            "nextLink": next,
        })
        .to_string()
    }

    #[tokio::test]
    async fn list_follows_next_link_on_the_vault_origin_only() {
        let mut server = mockito::Server::new_async().await;
        let _p1 = server
            .mock("GET", "/secrets")
            .match_query(mockito::Matcher::UrlEncoded(
                "maxresults".into(),
                "25".into(),
            ))
            .with_body(page(
                &["a"],
                Some(format!("{}/secrets?page=2", server.url())),
            ))
            .create_async()
            .await;
        let _p2 = server
            .mock("GET", "/secrets")
            .match_query(mockito::Matcher::UrlEncoded("page".into(), "2".into()))
            .with_body(page(&["b"], Some("https://attacker.example/steal".into())))
            .create_async()
            .await;

        let backend = backend_for(&server).await;
        let err = backend.list_secrets("").await.unwrap_err().to_string();
        assert!(err.contains("refusing to send the access token"), "{err}");
    }
}
