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
///
/// Redirects are followed (at most 10) only to destinations that
/// [`require_secure_url`] would accept: a 307/308 replays the request body
/// (a Vault login, a new password), so an https:// endpoint must not be able
/// to bounce it to plain http://.
pub fn build_http_client(timeout_secs: u64) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if !transport_is_secure(attempt.url()) {
                let origin = attempt.url().origin().ascii_serialization();
                attempt.error(format!(
                    "refusing to follow a redirect to plain http:// ({origin})"
                ))
            } else {
                attempt.follow()
            }
        }))
        .build()
        .context("Failed to build HTTP client")
}

/// The rule behind [`require_secure_url`], without logging: https://, or
/// http:// to loopback, or any http:// when the opt-out is set.
fn transport_is_secure(url: &reqwest::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => is_loopback(url) || std::env::var(ALLOW_INSECURE_HTTP_ENV).as_deref() == Ok("1"),
        _ => false,
    }
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
    // Never echo `url` itself: webhook URLs often carry their token in the
    // path, and any URL may hold user:password. Messages show only the origin.
    let parsed =
        reqwest::Url::parse(url).map_err(|e| anyhow::anyhow!("Invalid {} URL: {}", what, e))?;
    let origin = parsed.origin().ascii_serialization();
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(&parsed) => Ok(()),
        "http" if std::env::var(ALLOW_INSECURE_HTTP_ENV).as_deref() == Ok("1") => {
            tracing::warn!(
                "{} uses plain http:// ({}); credentials and secrets are sent unencrypted \
                 because {}=1 is set",
                what,
                origin,
                ALLOW_INSECURE_HTTP_ENV
            );
            Ok(())
        }
        "http" => anyhow::bail!(
            "{} uses plain http:// ({}), which would send credentials and secrets \
             unencrypted. Use https://, or set {}=1 if this endpoint is on a trusted \
             private network. http:// is always allowed for localhost.",
            what,
            origin,
            ALLOW_INSECURE_HTTP_ENV
        ),
        other => anyhow::bail!("{} URL must use https:// (got {}://)", what, other),
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
    fn errors_never_contain_the_path_query_or_credentials() {
        for url in [
            "http://hooks.internal/services/T000/B000/s3cr3tT0ken",
            "http://user:p4ssw0rd@vault.internal:8200/",
            "http://vault.internal/?token=s3cr3tT0ken",
            "ftp://user:p4ssw0rd@vault.internal/s3cr3tT0ken",
            "not a url s3cr3tT0ken",
        ] {
            let err = format!("{:#}", require_secure_url(url, "Webhook").unwrap_err());
            assert!(
                !err.contains("s3cr3tT0ken") && !err.contains("p4ssw0rd"),
                "{err}"
            );
        }
        let err = require_secure_url("http://hooks.internal/x", "Webhook").unwrap_err();
        assert!(err.to_string().contains("http://hooks.internal"), "{err}");
    }

    #[tokio::test]
    async fn redirects_to_remote_http_are_not_followed() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/login")
            .with_status(307)
            .with_header("location", "http://attacker.example/login")
            .create_async()
            .await;
        let client = build_http_client(5).unwrap();
        let err = client
            .post(format!("{}/login", server.url()))
            .body("secret_id=s3cr3t")
            .send()
            .await
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("refusing to follow a redirect"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn redirects_to_https_or_loopback_are_followed() {
        let mut server = mockito::Server::new_async().await;
        let _a = server
            .mock("GET", "/a")
            .with_status(302)
            .with_header("location", &format!("{}/b", server.url()))
            .create_async()
            .await;
        let _b = server
            .mock("GET", "/b")
            .with_body("ok")
            .create_async()
            .await;
        let resp = build_http_client(5)
            .unwrap()
            .get(format!("{}/a", server.url()))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.text().await.unwrap(), "ok");
    }

    #[test]
    fn other_schemes_and_garbage_are_rejected() {
        assert!(require_secure_url("ftp://vault.example.com", "Vault").is_err());
        assert!(require_secure_url("not a url", "Vault").is_err());
    }
}
