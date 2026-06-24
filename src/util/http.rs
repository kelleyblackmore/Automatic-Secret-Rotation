use anyhow::{Context, Result};

/// Assert an HTTP response is 2xx; on failure consume the body and bail with context.
///
/// Returns the response unchanged on success so callers can chain `.json()`.
pub async fn require_success(resp: reqwest::Response, context: &str) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    anyhow::bail!("{} (HTTP {}): {}", context, status, body)
}

/// Build an `reqwest::Client` with a fixed timeout and default TLS settings.
pub fn build_http_client(timeout_secs: u64) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .context("Failed to build HTTP client")
}
