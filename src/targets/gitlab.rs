#![cfg(feature = "gitlab")]

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::config::GitLabTargetConfig;
use crate::targets::target::Target;

const DEFAULT_GITLAB_URL: &str = "https://gitlab.com";

pub struct GitLabTarget {
    project_id: String,
    variable_key: String,
    base_url: String,
    token: String,
    masked: bool,
    protected: bool,
    client: reqwest::Client,
}

impl GitLabTarget {
    pub async fn new(config: &GitLabTargetConfig) -> Result<Self> {
        let token = config
            .token
            .clone()
            .or_else(|| std::env::var("GITLAB_TOKEN").ok())
            .context("GitLab token not set — provide token in config or set GITLAB_TOKEN")?;

        let base_url = config
            .gitlab_url
            .as_deref()
            .unwrap_or(DEFAULT_GITLAB_URL)
            .trim_end_matches('/')
            .to_string();

        // Percent-encode path separator so "group/project" → "group%2Fproject"
        let project_id = config.project_id.replace('/', "%2F");

        Ok(Self {
            project_id,
            variable_key: config.variable_key.clone(),
            base_url,
            token,
            masked: config.masked.unwrap_or(false),
            protected: config.protected.unwrap_or(false),
            client: crate::util::http::build_http_client(30)?,
        })
    }
}

#[async_trait]
impl Target for GitLabTarget {
    async fn update_password(&self, _username: &str, new_password: &str) -> Result<()> {
        let update_url = format!(
            "{}/api/v4/projects/{}/variables/{}",
            self.base_url, self.project_id, self.variable_key
        );
        let update_body = serde_json::json!({
            "value": new_password,
            "masked": self.masked,
            "protected": self.protected,
        });

        let resp = self
            .client
            .put(&update_url)
            .header("PRIVATE-TOKEN", &self.token)
            .json(&update_body)
            .send()
            .await
            .context("Failed to send GitLab variable PUT request")?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            // Variable doesn't exist yet — create it with POST
            let create_url = format!(
                "{}/api/v4/projects/{}/variables",
                self.base_url, self.project_id
            );
            let create_body = serde_json::json!({
                "key": self.variable_key,
                "value": new_password,
                "masked": self.masked,
                "protected": self.protected,
            });
            let create_resp = self
                .client
                .post(&create_url)
                .header("PRIVATE-TOKEN", &self.token)
                .json(&create_body)
                .send()
                .await
                .context("Failed to create GitLab CI/CD variable")?;
            crate::util::http::require_success(create_resp, "GitLab variable create").await?;
        } else {
            crate::util::http::require_success(resp, "GitLab variable update").await?;
        }

        Ok(())
    }

    async fn verify_connection(
        &self,
        _username: &str,
        _password: &str,
        _extra: Option<&str>,
    ) -> Result<()> {
        // Verify the token works by fetching the project info
        let url = format!("{}/api/v4/projects/{}", self.base_url, self.project_id);
        let resp = self
            .client
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .send()
            .await
            .context("Failed to verify GitLab connection")?;
        crate::util::http::require_success(resp, "GitLab connection verify").await?;
        Ok(())
    }

    fn target_type(&self) -> &'static str {
        "GitLab"
    }

    fn requires_username(&self) -> bool {
        false
    }
}
