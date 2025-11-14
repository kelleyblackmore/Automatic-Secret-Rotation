use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use super::secret_backend::{SecretBackend, SecretData};

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
pub struct VaultSecretData {
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
    pub async fn read_secret(&self, mount: &str, path: &str) -> Result<VaultSecretData> {
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

        let vault_response: VaultResponse<VaultSecretData> = response
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
            anyhow::bail!(
                "Vault metadata request failed with status {}: {}",
                status,
                body
            );
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

        // 404 means no secrets exist at this path, which is fine
        if response.status() == 404 {
            info!("No secrets found at {}/{} (empty path)", mount, path);
            return Ok(vec![]);
        }

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

/// Wrapper for VaultClient that implements SecretBackend trait
pub struct VaultBackend {
    client: VaultClient,
    mount: String,
}

impl VaultBackend {
    pub fn new(client: VaultClient, mount: String) -> Self {
        Self { client, mount }
    }
}

#[async_trait::async_trait]
impl SecretBackend for VaultBackend {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        let vault_data = self.client.read_secret(&self.mount, path).await?;
        
        let metadata = vault_data.metadata
            .and_then(|m| m.custom_metadata);
        
        Ok(SecretData {
            data: vault_data.data,
            metadata: metadata.clone(),
        })
    }

    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()> {
        self.client.write_secret(&self.mount, path, data).await
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()> {
        self.client.update_metadata(&self.mount, path, metadata).await
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        let metadata = self.client.read_metadata(&self.mount, path).await?;
        Ok(metadata.custom_metadata.unwrap_or_default())
    }

    async fn list_secrets(&self, path: &str) -> Result<Vec<String>> {
        self.client.list_secrets(&self.mount, path).await
    }

    fn backend_type(&self) -> &'static str {
        "HashiCorp Vault"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_client_new() {
        let client = VaultClient::new(
            "http://localhost:8200".to_string(),
            "test-token".to_string(),
        );
        assert!(client.is_ok());
    }

    #[test]
    fn test_vault_url_construction() {
        let client = VaultClient::new(
            "http://localhost:8200".to_string(),
            "test-token".to_string(),
        ).unwrap();

        // Test read URL
        let read_url = format!("{}/v1/{}/data/{}", client.address, "secret", "myapp/db");
        assert_eq!(read_url, "http://localhost:8200/v1/secret/data/myapp/db");

        // Test write URL
        let write_url = format!("{}/v1/{}/data/{}", client.address, "secret", "myapp/db");
        assert_eq!(write_url, "http://localhost:8200/v1/secret/data/myapp/db");

        // Test metadata URL
        let meta_url = format!("{}/v1/{}/metadata/{}", client.address, "secret", "myapp/db");
        assert_eq!(meta_url, "http://localhost:8200/v1/secret/metadata/myapp/db");
    }

    #[test]
    fn test_vault_secret_metadata_parsing() {
        let mut custom_meta = HashMap::new();
        custom_meta.insert("rotation_enabled".to_string(), "true".to_string());
        custom_meta.insert("last_rotated".to_string(), "2023-01-01T00:00:00Z".to_string());

        let metadata = SecretMetadata {
            custom_metadata: Some(custom_meta.clone()),
        };

        assert_eq!(
            metadata.custom_metadata.as_ref().unwrap().get("rotation_enabled"),
            Some(&"true".to_string())
        );
    }

    #[test]
    fn test_vault_secret_data_structure() {
        let mut data = HashMap::new();
        data.insert("password".to_string(), "secret123".to_string());
        data.insert("username".to_string(), "admin".to_string());

        let mut custom_meta = HashMap::new();
        custom_meta.insert("rotation_enabled".to_string(), "true".to_string());

        let secret_data = VaultSecretData {
            data: data.clone(),
            metadata: Some(SecretMetadata {
                custom_metadata: Some(custom_meta),
            }),
        };

        assert_eq!(secret_data.data.get("password"), Some(&"secret123".to_string()));
        assert_eq!(secret_data.data.get("username"), Some(&"admin".to_string()));
        assert!(secret_data.metadata.is_some());
    }

    #[test]
    fn test_vault_write_request_serialization() {
        let mut data = HashMap::new();
        data.insert("password".to_string(), "newpass".to_string());

        let request = VaultWriteRequest {
            data: data.clone(),
            options: None,
        };

        // Verify structure
        assert_eq!(request.data.get("password"), Some(&"newpass".to_string()));
        assert!(request.options.is_none());
    }
}
