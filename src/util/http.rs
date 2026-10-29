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

/// Opt-out for [`require_secure_url`], for endpoints on a trusted private
/// network that only speak plain HTTP.
pub const ALLOW_INSECURE_HTTP_ENV: &str = "ASR_ALLOW_INSECURE_HTTP";

/// Refuse a URL that would send credentials or secrets in cleartext.
///
/// `https://` is always accepted. `http://` is accepted only for loopback
/// hosts (`localhost`, `127.0.0.0/8`, `::1`), where nothing crosses the
/// network, unless `ASR_ALLOW_INSECURE_HTTP=1` is set, which allows it with a
/// warning. `what` names the endpoint and what it carries, for the error.
pub fn require_secure_url(url: &str, what: &str) -> Result<()> {
    let parsed =
        reqwest::Url::parse(url).with_context(|| format!("Invalid {} URL: {:?}", what, url))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        "http" if std::env::var(ALLOW_INSECURE_HTTP_ENV).as_deref() == Ok("1") => {
            tracing::warn!(
                "{} uses plain http:// ({}); credentials and secrets are sent unencrypted \
                 because {}=1 is set",
                what,
                url,
                ALLOW_INSECURE_HTTP_ENV
            );
            Ok(())
        }
        "http" => anyhow::bail!(
            "{} uses plain http:// ({}), which would send credentials and secrets \
             unencrypted. Use https://, or set {}=1 if this endpoint is on a trusted \
             private network. http:// is always allowed for localhost.",
            what,
            url,
            ALLOW_INSECURE_HTTP_ENV
        ),
        other => anyhow::bail!("{} URL must use https:// (got {}://): {}", what, other, url),
    }
}

fn is_loopback(url: &reqwest::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests must not set ASR_ALLOW_INSECURE_HTTP: tests run in
    // parallel and share the process environment.

    #[test]
    fn https_is_always_allowed() {
        assert!(require_secure_url("https://vault.example.com:8200", "Vault").is_ok());
    }

    #[test]
    fn http_is_allowed_only_for_loopback() {
        for ok in [
            "http://127.0.0.1:8200",
            "http://127.1.2.3",
            "http://localhost:8200",
            "http://LOCALHOST",
            "http://[::1]:8200",
        ] {
            assert!(require_secure_url(ok, "Vault").is_ok(), "{ok}");
        }
        for bad in [
            "http://vault.internal:8200",
            "http://10.0.0.5:8200",
            "http://localhost.evil.com",
            "http://127.0.0.1.nip.io",
        ] {
            let err = require_secure_url(bad, "Vault").unwrap_err().to_string();
            assert!(err.contains("unencrypted"), "{bad}: {err}");
        }
    }

    #[test]
    fn other_schemes_and_garbage_are_rejected() {
        assert!(require_secure_url("ftp://vault.example.com", "Vault").is_err());
        assert!(require_secure_url("not a url", "Vault").is_err());
    }
}
