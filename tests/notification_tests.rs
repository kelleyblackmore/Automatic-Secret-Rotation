use secret_rotator::config::NotificationConfig;
use secret_rotator::notification::NotificationClient;

fn cfg(webhook: Option<&str>, events: &[&str]) -> NotificationConfig {
    NotificationConfig {
        webhook_url: webhook.map(|s| s.to_string()),
        auth_header: None,
        events: events.iter().map(|s| s.to_string()).collect(),
    }
}

fn cfg_with_auth(webhook: &str, auth: &str, events: &[&str]) -> NotificationConfig {
    NotificationConfig {
        webhook_url: Some(webhook.to_string()),
        auth_header: Some(auth.to_string()),
        events: events.iter().map(|s| s.to_string()).collect(),
    }
}

// ---------------------------------------------------------------------------
// should_notify
// ---------------------------------------------------------------------------

#[test]
fn test_should_notify_false_when_no_webhook() {
    let client = NotificationClient::new(&cfg(None, &["rotate", "flag", "scan"]));
    assert!(!client.should_notify("rotate"));
    assert!(!client.should_notify("flag"));
    assert!(!client.should_notify("scan"));
}

#[test]
fn test_should_notify_false_for_event_not_in_list() {
    let client = NotificationClient::new(&cfg(
        Some("http://example.com/hook"),
        &["rotate"],
    ));
    assert!(!client.should_notify("flag"));
    assert!(!client.should_notify("scan"));
    assert!(!client.should_notify("unknown_event"));
}

#[test]
fn test_should_notify_true_for_configured_event() {
    let client = NotificationClient::new(&cfg(
        Some("http://example.com/hook"),
        &["rotate", "flag", "scan"],
    ));
    assert!(client.should_notify("rotate"));
    assert!(client.should_notify("flag"));
    assert!(client.should_notify("scan"));
}

#[test]
fn test_is_enabled_reflects_webhook_presence() {
    let disabled = NotificationClient::new(&cfg(None, &["rotate"]));
    let enabled = NotificationClient::new(&cfg(Some("http://example.com/hook"), &["rotate"]));
    assert!(!disabled.is_enabled());
    assert!(enabled.is_enabled());
}

// ---------------------------------------------------------------------------
// notify_rotate — no-op paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_notify_rotate_no_op_when_no_webhook() {
    let client = NotificationClient::new(&cfg(None, &["rotate"]));
    // Should return Ok without making any network call
    let result = client.notify_rotate("app/db", "vault", "success", None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_notify_rotate_no_op_when_event_not_in_list() {
    // webhook is set but "rotate" not in events list
    let client = NotificationClient::new(&cfg(
        Some("http://127.0.0.1:1/unreachable"),
        &["flag"],
    ));
    let result = client.notify_rotate("app/db", "vault", "success", None).await;
    assert!(result.is_ok(), "should skip without error: {:?}", result);
}

// ---------------------------------------------------------------------------
// notify_rotate — HTTP interaction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_notify_rotate_posts_to_webhook() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/hook")
        .with_status(200)
        .create_async()
        .await;

    let url = format!("{}/hook", server.url());
    let client = NotificationClient::new(&cfg(Some(&url), &["rotate"]));
    client
        .notify_rotate("app/db", "vault", "success", None)
        .await
        .unwrap();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_notify_rotate_swallows_non_2xx_response() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/hook")
        .with_status(503)
        .create_async()
        .await;

    let url = format!("{}/hook", server.url());
    let client = NotificationClient::new(&cfg(Some(&url), &["rotate"]));
    // 503 must not bubble up as an error
    let result = client.notify_rotate("app/db", "vault", "success", None).await;
    assert!(result.is_ok(), "non-2xx should be swallowed: {:?}", result);
}

#[tokio::test]
async fn test_notify_rotate_includes_error_field() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/hook")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "event": "rotated",
            "status": "failed",
            "error": "connection refused",
        })))
        .with_status(200)
        .create_async()
        .await;

    let url = format!("{}/hook", server.url());
    let client = NotificationClient::new(&cfg(Some(&url), &["rotate"]));
    client
        .notify_rotate("app/db", "vault", "failed", Some("connection refused"))
        .await
        .unwrap();

    mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// notify_flag
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_notify_flag_no_op_when_disabled() {
    let client = NotificationClient::new(&cfg(None, &["flag"]));
    let result = client.notify_flag("app/cert", "vault", 12).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_notify_flag_posts_correct_event() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/hook")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "event": "flagged",
            "path": "app/cert",
            "rotation_period_months": 6,
        })))
        .with_status(200)
        .create_async()
        .await;

    let url = format!("{}/hook", server.url());
    let client = NotificationClient::new(&cfg(Some(&url), &["flag"]));
    client.notify_flag("app/cert", "vault", 6).await.unwrap();

    mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// notify_scan
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_notify_scan_no_op_when_disabled() {
    let client = NotificationClient::new(&cfg(None, &["scan"]));
    let result = client.notify_scan("", "vault", 3).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_notify_scan_posts_correct_event() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/hook")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "event": "scanned",
            "secrets_due": 5,
        })))
        .with_status(200)
        .create_async()
        .await;

    let url = format!("{}/hook", server.url());
    let client = NotificationClient::new(&cfg(Some(&url), &["scan"]));
    client.notify_scan("app", "vault", 5).await.unwrap();

    mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// Auth header forwarding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_notify_sends_auth_header_when_configured() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/hook")
        .match_header("Authorization", "Bearer super-secret-token")
        .with_status(200)
        .create_async()
        .await;

    let url = format!("{}/hook", server.url());
    let client = NotificationClient::new(&cfg_with_auth(
        &url,
        "Bearer super-secret-token",
        &["rotate"],
    ));
    client
        .notify_rotate("path", "vault", "success", None)
        .await
        .unwrap();

    mock.assert_async().await;
}
