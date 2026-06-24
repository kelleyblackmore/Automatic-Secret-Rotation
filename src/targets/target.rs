use anyhow::Result;

/// Trait for password update targets (databases, APIs, applications, etc.)
#[async_trait::async_trait]
pub trait Target: Send + Sync {
    /// Update the secret value in the target system.
    ///
    /// `username` is empty for targets that don't have a user concept
    /// (GitLab CI/CD variables, GitHub Actions secrets/variables, etc.).
    async fn update_password(&self, username: &str, new_password: &str) -> Result<()>;

    /// Verify that the new secret is accepted by the target.
    async fn verify_connection(
        &self,
        username: &str,
        password: &str,
        database: Option<&str>,
    ) -> Result<()>;

    /// Get the target type name for display purposes.
    fn target_type(&self) -> &'static str;

    /// Whether this target needs a username to operate.
    ///
    /// Defaults to `true` (databases, REST APIs). Token-based targets
    /// (GitLab CI/CD variables, GitHub Actions secrets/variables) override
    /// this to return `false` so rotation works without `--target-username`.
    fn requires_username(&self) -> bool {
        true
    }
}
