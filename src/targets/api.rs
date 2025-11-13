use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, info};

use crate::config::ApiTargetConfig;
use crate::targets::target::Target;

/// API-based target for password updates via REST API
pub struct ApiTarget {
    config: Arc<ApiTargetConfig>,
    client: Client,
}

impl ApiTarget {
    /// Create a new ApiTarget
    pub async fn new(config: &ApiTargetConfig) -> Result<Self> {
        info!("Creating API target for: {}", config.base_url);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            config: Arc::new(config.clone()),
            client,
        })
    }

    /// Build the full URL for password update endpoint
    fn build_url(&self, username: &str) -> String {
        // Replace {username} placeholder if present
        let url = self.config.endpoint.replace("{username}", username);
        
        if url.starts_with("http://") || url.starts_with("https://") {
            url
        } else {
            format!("{}/{}", self.config.base_url.trim_end_matches('/'), url.trim_start_matches('/'))
        }
    }
}

#[async_trait::async_trait]
impl Target for ApiTarget {
    async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        info!("Updating password via API for user: {}", username);

        let url = self.build_url(username);
        debug!("Calling API endpoint: {}", url);

        // Build request body based on config
        let mut body = json!({});
        
        // Set username field
        if let Some(ref username_field) = self.config.username_field {
            body[username_field] = json!(username);
        }
        
        // Set password field
        body[&self.config.password_field] = json!(new_password);

        // Add any additional fields from config
        if let Some(ref additional_fields) = self.config.additional_fields {
            for (key, value) in additional_fields {
                body[key] = json!(value);
            }
        }

        // Parse HTTP method
        let method = match self.config.method.to_uppercase().as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            "DELETE" => reqwest::Method::DELETE,
            _ => reqwest::Method::POST,
        };

        // Build request
        let mut request = self.client
            .request(method, &url)
            .json(&body);

        // Add authentication headers if configured
        if let Some(ref auth_header) = self.config.auth_header {
            request = request.header("Authorization", auth_header);
        }

        // Add custom headers if configured
        if let Some(ref headers) = self.config.headers {
            for (key, value) in headers {
                request = request.header(key, value);
            }
        }

        // Send request
        let response = request
            .send()
            .await
            .context("Failed to send API request")?;

        // Check response status
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            anyhow::bail!(
                "API request failed with status {}: {}",
                status,
                error_text
            );
        }

        info!("Successfully updated password via API for user: {}", username);
        Ok(())
    }

    async fn verify_connection(&self, _username: &str, _password: &str, _database: Option<&str>) -> Result<()> {
        // API targets may not support verification, or it could be done via a separate endpoint
        // For now, we'll skip verification for API targets
        info!("Verification not supported for API targets");
        Ok(())
    }

    fn target_type(&self) -> &'static str {
        "api"
    }
}

