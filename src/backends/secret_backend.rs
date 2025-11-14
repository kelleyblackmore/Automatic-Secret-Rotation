use anyhow::Result;
use std::collections::HashMap;

/// Common data structure for secrets across backends
#[derive(Debug, Clone)]
pub struct SecretData {
    pub data: HashMap<String, String>,
    pub metadata: Option<HashMap<String, String>>,
}

/// Trait for secret management backends (Vault, AWS Secrets Manager, etc.)
#[async_trait::async_trait]
pub trait SecretBackend: Send + Sync {
    /// Read a secret from the backend
    async fn read_secret(&self, path: &str) -> Result<SecretData>;

    /// Write a secret to the backend
    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()>;

    /// Update metadata for a secret
    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()>;

    /// Read metadata for a secret
    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>>;

    /// List secrets at a path
    async fn list_secrets(&self, path: &str) -> Result<Vec<String>>;

    /// Get the backend type name for display purposes
    fn backend_type(&self) -> &'static str;
}
