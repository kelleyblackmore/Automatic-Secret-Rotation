use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// HashiCorp Vault client
#[derive(Clone)]
pub struct VaultClient {
    client: Client,
    address: String,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub custom_metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretData {
    pub data: HashMap<String, String>,
    pub metadata: Option<SecretMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultResponse<T> {
    data: T,
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultWriteRequest {
    data: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<HashMap<String, String>>,
}

impl VaultClient {
    /// Create a new Vault client
    pub fn new(address: String, token: String) -> Result<Self> {
        let client = Client::builder()
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            address,
            token,
        })
    }

    /// Read a secret from Vault KV v2
    pub async fn read_secret(&self, mount: &str, path: &str) -> Result<SecretData> {
        let url = format!("{}/v1/{}/data/{}", self.address, mount, path);
        debug!("Reading secret from: {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .context("Failed to read secret from Vault")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Vault request failed with status {}: {}", status, body);
        }

        let vault_response: VaultResponse<SecretData> = response
            .json()
            .await
            .context("Failed to parse Vault response")?;

        Ok(vault_response.data)
    }

    /// Write a secret to Vault KV v2
    pub async fn write_secret(
        &self,
        mount: &str,
        path: &str,
        data: HashMap<String, String>,
    ) -> Result<()> {
        let url = format!("{}/v1/{}/data/{}", self.address, mount, path);
        debug!("Writing secret to: {}", url);

        let request_body = VaultWriteRequest {
            data,
            options: None,
        };

        let response = self
            .client
            .post(&url)
            .header("X-Vault-Token", &self.token)
            .json(&request_body)
            .send()
            .await
            .context("Failed to write secret to Vault")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Vault write failed with status {}: {}", status, body);
        }

        info!("Successfully wrote secret to {}/{}", mount, path);
        Ok(())
    }

    /// Update secret metadata
    pub async fn update_metadata(
        &self,
        mount: &str,
        path: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        let url = format!("{}/v1/{}/metadata/{}", self.address, mount, path);
        debug!("Updating metadata at: {}", url);

        let mut body = HashMap::new();
        body.insert("custom_metadata", metadata);

        let response = self
            .client
            .post(&url)
            .header("X-Vault-Token", &self.token)
            .json(&body)
            .send()
            .await
            .context("Failed to update metadata")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Metadata update failed with status {}: {}", status, body);
        }

        info!("Successfully updated metadata for {}/{}", mount, path);
        Ok(())
    }

    /// Read secret metadata
    pub async fn read_metadata(&self, mount: &str, path: &str) -> Result<SecretMetadata> {
        let url = format!("{}/v1/{}/metadata/{}", self.address, mount, path);
        debug!("Reading metadata from: {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .context("Failed to read metadata from Vault")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Vault metadata request failed with status {}: {}", status, body);
        }

        let vault_response: VaultResponse<SecretMetadata> = response
            .json()
            .await
            .context("Failed to parse Vault metadata response")?;

        Ok(vault_response.data)
    }

    /// List secrets in a path
    pub async fn list_secrets(&self, mount: &str, path: &str) -> Result<Vec<String>> {
        let url = format!("{}/v1/{}/metadata/{}", self.address, mount, path);
        debug!("Listing secrets at: {}", url);

        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"LIST").unwrap(), &url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .context("Failed to list secrets from Vault")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Vault list request failed with status {}: {}", status, body);
        }

        #[derive(Deserialize)]
        struct ListData {
            keys: Vec<String>,
        }

        let vault_response: VaultResponse<ListData> = response
            .json()
            .await
            .context("Failed to parse Vault list response")?;

        Ok(vault_response.data.keys)
    }
}
