use anyhow::Result;

/// Trait for password update targets (databases, APIs, applications, etc.)
#[async_trait::async_trait]
pub trait Target: Send + Sync {
    /// Update password for a user/account in the target system
    async fn update_password(&self, username: &str, new_password: &str) -> Result<()>;

    /// Verify that the new password works (optional, may not be supported by all targets)
    async fn verify_connection(
        &self,
        username: &str,
        password: &str,
        database: Option<&str>,
    ) -> Result<()>;

    /// Get the target type name for display purposes
    fn target_type(&self) -> &'static str;
}
