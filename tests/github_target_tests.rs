#![cfg(feature = "github")]

use anyhow::Result;
use secret_rotator::config::GitHubTargetConfig;
use secret_rotator::targets::{GitHubTarget, Target};

fn variable_config(server_url: &str, var: &str) -> GitHubTargetConfig {
    GitHubTargetConfig {
        owner: "myorg".to_string(),
        repo: "myrepo".to_string(),
        secret_name: None,
        variable_name: Some(var.to_string()),
        token: Some("ghp_test_token".to_string()),
        token_path: None,
        env_name: None,
        api_url: Some(server_url.to_string()),
    }
}

fn secret_config(server_url: &str, secret: &str) -> GitHubTargetConfig {
    GitHubTargetConfig {
        owner: "myorg".to_string(),
        repo: "myrepo".to_string(),
        secret_name: Some(secret.to_string()),
        variable_name: None,
        token: Some("ghp_test_token".to_string()),
        token_path: None,
        env_name: None,
        api_url: Some(server_url.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Construction validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_github_requires_secret_name_or_variable_name() {
    let config = GitHubTargetConfig {
        owner: "myorg".to_string(),
        repo: "myrepo".to_string(),
        secret_name: None,
        variable_name: None,
        token: Some("ghp_tok".to_string()),
        token_path: None,
        env_name: None,
        api_url: None,
    };
    let result = GitHubTarget::new(&config).await;
    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("secret_name or variable_name"),
        "error should mention the two fields: {msg}"
    );
}

#[tokio::test]
async fn test_github_secret_name_and_variable_name_are_mutually_exclusive() {
    let config = GitHubTargetConfig {
        owner: "myorg".to_string(),
        repo: "myrepo".to_string(),
        secret_name: Some("MY_SECRET".to_string()),
        variable_name: Some("MY_VAR".to_string()),
        token: Some("ghp_tok".to_string()),
        token_path: None,
        env_name: None,
        api_url: None,
    };
    let result = GitHubTarget::new(&config).await;
    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("mutually exclusive"),
        "error should say mutually exclusive: {msg}"
    );
}

#[tokio::test]
async fn test_github_requires_token() {
    // Temporarily clear GITHUB_TOKEN so the test is deterministic regardless of environment.
    let saved = std::env::var("GITHUB_TOKEN").ok();
    std::env::remove_var("GITHUB_TOKEN");

    let config = GitHubTargetConfig {
        owner: "myorg".to_string(),
        repo: "myrepo".to_string(),
        secret_name: None,
        variable_name: Some("MY_VAR".to_string()),
        token: None,
        token_path: None,
        env_name: None,
        api_url: None,
    };
    let result = GitHubTarget::new(&config).await;

    if let Some(v) = saved {
        std::env::set_var("GITHUB_TOKEN", v);
    }

    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.to_lowercase().contains("token"),
        "error should mention missing token: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Trait properties
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_github_does_not_require_username() -> Result<()> {
    let server = mockito::Server::new_async().await;
    let target = GitHubTarget::new(&variable_config(&server.url(), "MY_VAR")).await?;
    assert!(!target.requires_username());
    Ok(())
}

#[tokio::test]
async fn test_github_target_type_label() -> Result<()> {
    let server = mockito::Server::new_async().await;
    let target = GitHubTarget::new(&variable_config(&server.url(), "MY_VAR")).await?;
    assert_eq!(target.target_type(), "GitHub");
    Ok(())
}

// ---------------------------------------------------------------------------
// update_password — variables
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_github_update_variable_creates_when_not_found() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let get_mock = server
        .mock("GET", "/repos/myorg/myrepo/actions/variables/NEW_VAR")
        .with_status(404)
        .with_body(r#"{"message":"Not Found"}"#)
        .create_async()
        .await;

    let post_mock = server
        .mock("POST", "/repos/myorg/myrepo/actions/variables")
        .with_status(201)
        .create_async()
        .await;

    let target = GitHubTarget::new(&variable_config(&server.url(), "NEW_VAR")).await?;
    target.update_password("", "new-value").await?;

    get_mock.assert_async().await;
    post_mock.assert_async().await;
    Ok(())
}

#[tokio::test]
async fn test_github_update_variable_patches_existing() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let get_mock = server
        .mock("GET", "/repos/myorg/myrepo/actions/variables/EXISTING_VAR")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"name":"EXISTING_VAR","value":"old"}"#)
        .create_async()
        .await;

    let patch_mock = server
        .mock(
            "PATCH",
            "/repos/myorg/myrepo/actions/variables/EXISTING_VAR",
        )
        .with_status(204)
        .create_async()
        .await;

    let target = GitHubTarget::new(&variable_config(&server.url(), "EXISTING_VAR")).await?;
    target.update_password("", "new-value").await?;

    get_mock.assert_async().await;
    patch_mock.assert_async().await;
    Ok(())
}

#[tokio::test]
async fn test_github_update_variable_env_scoped_create() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let get_mock = server
        .mock(
            "GET",
            "/repos/myorg/myrepo/environments/production/variables/MY_VAR",
        )
        .with_status(404)
        .create_async()
        .await;

    let post_mock = server
        .mock(
            "POST",
            "/repos/myorg/myrepo/environments/production/variables",
        )
        .with_status(201)
        .create_async()
        .await;

    let config = GitHubTargetConfig {
        owner: "myorg".to_string(),
        repo: "myrepo".to_string(),
        secret_name: None,
        variable_name: Some("MY_VAR".to_string()),
        token: Some("ghp_tok".to_string()),
        token_path: None,
        env_name: Some("production".to_string()),
        api_url: Some(server.url()),
    };
    let target = GitHubTarget::new(&config).await?;
    target.update_password("", "val").await?;

    get_mock.assert_async().await;
    post_mock.assert_async().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// update_password — secrets (NaCl sealed-box path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_github_update_secret_fetches_pubkey_and_uploads() -> Result<()> {
    use secret_rotator::util::base64;

    let mut server = mockito::Server::new_async().await;

    // Use a real X25519 public key so the sealed-box encryption succeeds.
    // Curve25519 base point in little-endian (the "9" point).
    let pk_bytes: [u8; 32] = [
        0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    let pk_b64 = base64::encode(&pk_bytes);

    let pk_mock = server
        .mock("GET", "/repos/myorg/myrepo/actions/secrets/public-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(r#"{{"key_id":"key_abc123","key":"{pk_b64}"}}"#))
        .create_async()
        .await;

    let put_mock = server
        .mock("PUT", "/repos/myorg/myrepo/actions/secrets/MY_SECRET")
        .with_status(204)
        .create_async()
        .await;

    let target = GitHubTarget::new(&secret_config(&server.url(), "MY_SECRET")).await?;
    target.update_password("", "new-secret-value").await?;

    pk_mock.assert_async().await;
    put_mock.assert_async().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// verify_connection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_github_verify_connection_get_repo() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let get_mock = server
        .mock("GET", "/repos/myorg/myrepo")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":1,"name":"myrepo","full_name":"myorg/myrepo"}"#)
        .create_async()
        .await;

    let target = GitHubTarget::new(&variable_config(&server.url(), "MY_VAR")).await?;
    target.verify_connection("", "", None).await?;

    get_mock.assert_async().await;
    Ok(())
}

#[tokio::test]
async fn test_github_verify_connection_401_fails() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let _get = server
        .mock("GET", "/repos/myorg/myrepo")
        .with_status(401)
        .with_body(r#"{"message":"Unauthorized"}"#)
        .create_async()
        .await;

    let target = GitHubTarget::new(&variable_config(&server.url(), "MY_VAR")).await?;
    let result = target.verify_connection("", "", None).await;
    assert!(result.is_err(), "401 should produce an error");
    Ok(())
}
