use anyhow::{Context, Result};
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use aws_sdk_secretsmanager::types::Tag;
use serde_json;
use std::collections::HashMap;
use tracing::{debug, info};

use super::secret_backend::{SecretBackend, SecretData};

/// AWS Secrets Manager client
pub struct AwsSecretsClient {
    client: SecretsManagerClient,
    #[allow(dead_code)] // Kept for potential future use (logging, debugging)
    region: String,
}

impl AwsSecretsClient {
    /// Create a new AWS Secrets Manager client
    pub async fn new(region: Option<String>) -> Result<Self> {
        let region_str = region.unwrap_or_else(|| {
            std::env::var("AWS_REGION")
                .unwrap_or_else(|_| "us-east-1".to_string())
        });

        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = SecretsManagerClient::new(&config);

        Ok(Self {
            client,
            region: region_str.clone(),
        })
    }

    /// Convert AWS tags to metadata HashMap
    fn tags_to_metadata(&self, tags: &[Tag]) -> HashMap<String, String> {
        tags.iter()
            .filter_map(|tag| {
                tag.key()
                    .and_then(|k| tag.value().map(|v| (k.to_string(), v.to_string())))
            })
            .collect()
    }

    /// Convert metadata HashMap to AWS tags
    fn metadata_to_tags(&self, metadata: &HashMap<String, String>) -> Vec<Tag> {
        metadata
            .iter()
            .map(|(k, v)| Tag::builder().key(k).value(v).build())
            .collect()
    }
}

#[async_trait::async_trait]
impl SecretBackend for AwsSecretsClient {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        debug!("Reading secret from AWS Secrets Manager: {}", path);

        let response = self
            .client
            .get_secret_value()
            .secret_id(path)
            .send()
            .await
            .with_context(|| format!("Failed to read secret '{}' from AWS Secrets Manager", path))?;

        // Parse the secret string as JSON
        let secret_string = response
            .secret_string()
            .ok_or_else(|| anyhow::anyhow!("Secret '{}' has no string value", path))?;

        let data: HashMap<String, String> = serde_json::from_str(secret_string)
            .with_context(|| format!("Failed to parse secret '{}' as JSON", path))?;

        // Get tags for metadata
        let tags_response = self
            .client
            .describe_secret()
            .secret_id(path)
            .send()
            .await
            .ok();

        let metadata = tags_response
            .map(|r| self.tags_to_metadata(r.tags()))
            .unwrap_or_default();

        Ok(SecretData { data, metadata: Some(metadata) })
    }

    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()> {
        debug!("Writing secret to AWS Secrets Manager: {}", path);

        // Convert HashMap to JSON string
        let secret_string = serde_json::to_string(&data)
            .context("Failed to serialize secret data to JSON")?;

        // Check if secret exists
        let exists = self
            .client
            .describe_secret()
            .secret_id(path)
            .send()
            .await
            .is_ok();

        if exists {
            // Update existing secret
            self.client
                .update_secret()
                .secret_id(path)
                .secret_string(&secret_string)
                .send()
                .await
                .with_context(|| format!("Failed to update secret '{}' in AWS Secrets Manager", path))?;
            info!("Successfully updated secret '{}' in AWS Secrets Manager", path);
        } else {
            // Create new secret
            self.client
                .create_secret()
                .name(path)
                .secret_string(&secret_string)
                .send()
                .await
                .with_context(|| format!("Failed to create secret '{}' in AWS Secrets Manager", path))?;
            info!("Successfully created secret '{}' in AWS Secrets Manager", path);
        }

        Ok(())
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()> {
        debug!("Updating metadata for secret: {}", path);

        // Get existing tags
        let existing_tags = self
            .client
            .describe_secret()
            .secret_id(path)
            .send()
            .await
            .map(|r| self.tags_to_metadata(r.tags()))
            .unwrap_or_default();

        // Merge with new metadata
        let mut all_tags = existing_tags;
        all_tags.extend(metadata);

        // Convert to AWS tags
        let tags: Vec<Tag> = self.metadata_to_tags(&all_tags);

        // Update tags
        self.client
            .tag_resource()
            .secret_id(path)
            .set_tags(Some(tags))
            .send()
            .await
            .with_context(|| format!("Failed to update metadata for secret '{}'", path))?;

        info!("Successfully updated metadata for secret '{}'", path);
        Ok(())
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        debug!("Reading metadata for secret: {}", path);

        let response = self
            .client
            .describe_secret()
            .secret_id(path)
            .send()
            .await
            .with_context(|| format!("Failed to read metadata for secret '{}'", path))?;

        let metadata = self.tags_to_metadata(response.tags());

        Ok(metadata)
    }

    async fn list_secrets(&self, path: &str) -> Result<Vec<String>> {
        debug!("Listing secrets in AWS Secrets Manager with prefix: {}", path);

        let mut secrets = Vec::new();
        let mut next_token: Option<String> = None;

        loop {
            let mut request = self.client.list_secrets();

            if let Some(ref token) = next_token {
                request = request.set_next_token(Some(token.clone()));
            }

            let response = request
                .send()
                .await
                .context("Failed to list secrets from AWS Secrets Manager")?;

            for secret in response.secret_list() {
                if let Some(name) = secret.name() {
                    // Filter by prefix if path is specified
                    if path.is_empty() || name.starts_with(path) {
                        // If path is not empty, remove the prefix to match Vault behavior
                        let secret_name = if path.is_empty() {
                            name.to_string()
                        } else {
                            name.strip_prefix(path)
                                .and_then(|s| s.strip_prefix("/"))
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| name.to_string())
                        };
                        secrets.push(secret_name);
                    }
                }
            }

            next_token = response.next_token().map(|s| s.to_string());
            if next_token.is_none() {
                break;
            }
        }

        Ok(secrets)
    }

    fn backend_type(&self) -> &'static str {
        "AWS Secrets Manager"
    }
}

