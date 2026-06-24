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
        Ok(Self {
            vault_url: config.vault_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .context("Failed to build HTTP client")?,
        })
    }

    async fn bearer_token(&self) -> Result<String> {
        // Prefer an explicit env-var token (useful in CI)
        if let Ok(token) = std::env::var("AZURE_ACCESS_TOKEN") {
            return Ok(token);
        }

        // Fall back to Azure IMDS (works on Azure VMs / App Service / AKS)
        let resp = self.client
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
        struct ImdsToken { access_token: String }
        let token: ImdsToken = resp
            .json()
            .await
            .context("Failed to parse Azure IMDS token response")?;

        Ok(token.access_token)
    }

    fn path_to_name(path: &str) -> String {
        path.replace('/', "-")
    }

    fn name_to_path(name: &str) -> String {
        name.replace('-', "/")
    }

    async fn get_secret_bundle(&self, name: &str) -> Result<KvSecretBundle> {
        let token = self.bearer_token().await?;
        let url = format!(
            "{}/secrets/{}?api-version={}",
            self.vault_url, name, API_VERSION
        );
        let resp = self.client.get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .with_context(|| format!("Failed to GET secret: {}", name))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Azure Key Vault GET {} → {}: {}", name, status, body);
        }

        resp.json::<KvSecretBundle>()
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
        let mut data: HashMap<String, String> = if let Ok(map) =
            serde_json::from_str::<HashMap<String, String>>(&value)
        {
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

        let resp = self.client.put(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to PUT secret: {}", path))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Azure Key Vault PUT {} → {}: {}", path, status, text);
        }

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

        let resp = self.client.patch(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to PATCH secret metadata: {}", path))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Azure Key Vault PATCH {} → {}: {}", path, status, text);
        }

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
            let resp = self.client.get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .context("Failed to list Azure Key Vault secrets")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Azure Key Vault LIST → {}: {}", status, text);
            }

            let list: KvSecretList = resp.json().await.context("Failed to parse secret list")?;

            for item in list.value {
                // id is like https://vault.vault.azure.net/secrets/my-secret
                if let Some(name) = item.id.rsplit('/').next() {
                    if prefix.is_empty() || name.starts_with(&prefix) {
                        all.push(Self::name_to_path(name));
                    }
                }
            }

            match list.next_link {
                Some(next) => url = next,
                None => break,
            }
        }

        Ok(all)
    }

    fn backend_type(&self) -> &'static str {
        "Azure Key Vault"
    }
}
