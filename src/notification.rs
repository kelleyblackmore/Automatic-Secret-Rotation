use anyhow::Result;
use serde_json::json;
use tracing::warn;

use crate::config::NotificationConfig;

pub struct NotificationClient {
    webhook_url: Option<String>,
    auth_header: Option<String>,
    events: Vec<String>,
    client: reqwest::Client,
}

impl NotificationClient {
    pub fn new(config: &NotificationConfig) -> Self {
        Self {
            webhook_url: config.webhook_url.clone(),
            auth_header: config.auth_header.clone(),
            events: config.events.clone(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.webhook_url.is_some()
    }

    pub fn should_notify(&self, event: &str) -> bool {
        self.webhook_url.is_some() && self.events.iter().any(|e| e == event)
    }

    /// Send a webhook notification for a rotation event.
    pub async fn notify_rotate(
        &self,
        path: &str,
        backend: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        if !self.should_notify("rotate") {
            return Ok(());
        }

        let mut body = json!({
            "event": "rotated",
            "path": path,
            "backend": backend,
            "status": status,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "asr_version": env!("CARGO_PKG_VERSION"),
        });

        if let Some(err) = error {
            body["error"] = json!(err);
        }

        self.send(&body).await
    }

    /// Send a webhook notification for a flag event.
    pub async fn notify_flag(&self, path: &str, backend: &str, period_months: u32) -> Result<()> {
        if !self.should_notify("flag") {
            return Ok(());
        }

        let body = json!({
            "event": "flagged",
            "path": path,
            "backend": backend,
            "rotation_period_months": period_months,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "asr_version": env!("CARGO_PKG_VERSION"),
        });

        self.send(&body).await
    }

    /// Send a webhook notification for a scan event.
    pub async fn notify_scan(&self, path: &str, backend: &str, secrets_due: usize) -> Result<()> {
        if !self.should_notify("scan") {
            return Ok(());
        }

        let body = json!({
            "event": "scanned",
            "path": if path.is_empty() { "/" } else { path },
            "backend": backend,
            "secrets_due": secrets_due,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "asr_version": env!("CARGO_PKG_VERSION"),
        });

        self.send(&body).await
    }

    async fn send(&self, body: &serde_json::Value) -> Result<()> {
        let Some(ref url) = self.webhook_url else {
            return Ok(());
        };

        let mut request = self.client.post(url).json(body);
        if let Some(ref auth) = self.auth_header {
            request = request.header("Authorization", auth);
        }

        match request.send().await {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => {
                warn!(
                    "Webhook notification returned non-success status: {}",
                    resp.status()
                );
            }
            Err(e) => {
                // Log but don't fail the rotation on notification error
                warn!("Failed to send webhook notification: {}", e);
            }
        }

        Ok(())
    }
}
