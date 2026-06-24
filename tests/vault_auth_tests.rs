use secret_rotator::backends::VaultBackend;
use secret_rotator::config::{
    VaultAppRoleConfig, VaultConfig, VaultJwtConfig, VaultKubernetesConfig,
};
use tempfile::TempDir;

fn vault_token_config(address: &str, token: Option<&str>) -> VaultConfig {
    VaultConfig {
        address: address.to_string(),
        token: token.map(|t| t.to_string()),
        mount: "secret".to_string(),
        auth_method: "token".to_string(),
        approle: None,
        kubernetes: None,
        aws_iam: None,
        jwt: None,
    }
}

fn vault_auth_response_body() -> &'static str {
    r#"{"auth":{"client_token":"s.mock-token","renewable":true,"lease_duration":3600,"policies":["default"]}}"#
}

// ---------------------------------------------------------------------------
// Token auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_token_auth_from_config_field() {
    let config = vault_token_config("http://127.0.0.1:1", Some("s.direct-token"));
    let result = VaultBackend::from_config(&config).await;
    assert!(
        result.is_ok(),
        "token auth with config.token should succeed: {}",
        result.err().map(|e| e.to_string()).unwrap_or_default()
    );
}

#[tokio::test]
async fn test_token_auth_missing_fails() {
    // Ensure VAULT_TOKEN is not set for this test. If it is, the test is a no-op.
    if std::env::var("VAULT_TOKEN").is_ok() {
        return;
    }
    let config = vault_token_config("http://127.0.0.1:1", None);
    let result = VaultBackend::from_config(&config).await;
    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.to_lowercase().contains("token"),
        "error should mention token: {msg}"
    );
}

// ---------------------------------------------------------------------------
// AppRole auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_approle_auth_success() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/auth/approle/login")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_auth_response_body())
        .create_async()
        .await;

    let config = VaultConfig {
        address: server.url(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "approle".to_string(),
        approle: Some(VaultAppRoleConfig {
            role_id: "test-role-id".to_string(),
            secret_id: Some("test-secret-id".to_string()),
            secret_id_env: None,
            mount: "approle".to_string(),
        }),
        kubernetes: None,
        aws_iam: None,
        jwt: None,
    };

    let result = VaultBackend::from_config(&config).await;
    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "AppRole auth should succeed: {}",
        result.err().map(|e| e.to_string()).unwrap_or_default()
    );
}

#[tokio::test]
async fn test_approle_auth_custom_mount() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/auth/my-approle/login")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_auth_response_body())
        .create_async()
        .await;

    let config = VaultConfig {
        address: server.url(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "approle".to_string(),
        approle: Some(VaultAppRoleConfig {
            role_id: "role".to_string(),
            secret_id: Some("secret".to_string()),
            secret_id_env: None,
            mount: "my-approle".to_string(),
        }),
        kubernetes: None,
        aws_iam: None,
        jwt: None,
    };

    let result = VaultBackend::from_config(&config).await;
    mock.assert_async().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_approle_auth_server_403_fails() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/auth/approle/login")
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["permission denied"]}"#)
        .create_async()
        .await;

    let config = VaultConfig {
        address: server.url(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "approle".to_string(),
        approle: Some(VaultAppRoleConfig {
            role_id: "bad-role".to_string(),
            secret_id: Some("bad-secret".to_string()),
            secret_id_env: None,
            mount: "approle".to_string(),
        }),
        kubernetes: None,
        aws_iam: None,
        jwt: None,
    };

    let result = VaultBackend::from_config(&config).await;
    mock.assert_async().await;
    assert!(result.is_err(), "403 from Vault should produce an error");
}

#[tokio::test]
async fn test_approle_auth_missing_config_section_fails() {
    let config = VaultConfig {
        address: "http://127.0.0.1:1".to_string(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "approle".to_string(),
        approle: None,
        kubernetes: None,
        aws_iam: None,
        jwt: None,
    };

    let result = VaultBackend::from_config(&config).await;
    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("approle"),
        "error should mention the missing section: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Kubernetes auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_kubernetes_auth_success() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/auth/kubernetes/login")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_auth_response_body())
        .create_async()
        .await;

    let temp = TempDir::new().unwrap();
    let jwt_path = temp.path().join("sa-token");
    std::fs::write(&jwt_path, "eyJhbGciOiJSUzI1NiJ9.fake-jwt-payload.fake-sig").unwrap();

    let config = VaultConfig {
        address: server.url(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "kubernetes".to_string(),
        approle: None,
        kubernetes: Some(VaultKubernetesConfig {
            role: "asr-role".to_string(),
            sa_token_path: jwt_path.to_str().unwrap().to_string(),
            mount: "kubernetes".to_string(),
        }),
        aws_iam: None,
        jwt: None,
    };

    let result = VaultBackend::from_config(&config).await;
    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "Kubernetes auth should succeed: {}",
        result.err().map(|e| e.to_string()).unwrap_or_default()
    );
}

#[tokio::test]
async fn test_kubernetes_auth_missing_jwt_file_fails() {
    let config = VaultConfig {
        address: "http://127.0.0.1:1".to_string(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "kubernetes".to_string(),
        approle: None,
        kubernetes: Some(VaultKubernetesConfig {
            role: "asr-role".to_string(),
            sa_token_path: "/nonexistent/path/to/token".to_string(),
            mount: "kubernetes".to_string(),
        }),
        aws_iam: None,
        jwt: None,
    };

    let result = VaultBackend::from_config(&config).await;
    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("nonexistent") || msg.to_lowercase().contains("token"),
        "error should describe the missing file: {msg}"
    );
}

// ---------------------------------------------------------------------------
// JWT auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_jwt_auth_from_explicit_env_var() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/auth/jwt/login")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(vault_auth_response_body())
        .create_async()
        .await;

    // Use a distinctive env var name unlikely to be set in normal environments
    let env_key = "ASR_TEST_VAULT_JWT_TOKEN_AUTH_TEST";
    std::env::set_var(env_key, "eyJhbGciOiJSUzI1NiJ9.test-jwt.sig");

    let config = VaultConfig {
        address: server.url(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "jwt".to_string(),
        approle: None,
        kubernetes: None,
        aws_iam: None,
        jwt: Some(VaultJwtConfig {
            role: "my-role".to_string(),
            token_env: Some(env_key.to_string()),
            mount: "jwt".to_string(),
        }),
    };

    let result = VaultBackend::from_config(&config).await;
    std::env::remove_var(env_key);
    mock.assert_async().await;
    assert!(
        result.is_ok(),
        "JWT auth from env var should succeed: {}",
        result.err().map(|e| e.to_string()).unwrap_or_default()
    );
}

#[tokio::test]
async fn test_jwt_auth_missing_token_fails() {
    let env_key = "ASR_TEST_VAULT_JWT_MISSING_TOKEN";
    std::env::remove_var(env_key);

    let config = VaultConfig {
        address: "http://127.0.0.1:1".to_string(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "jwt".to_string(),
        approle: None,
        kubernetes: None,
        aws_iam: None,
        jwt: Some(VaultJwtConfig {
            role: "my-role".to_string(),
            token_env: Some(env_key.to_string()),
            mount: "jwt".to_string(),
        }),
    };

    let result = VaultBackend::from_config(&config).await;
    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains(env_key) || msg.to_lowercase().contains("token"),
        "error should mention the missing token env var: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Unknown auth method
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_unknown_auth_method_fails_with_helpful_message() {
    let config = VaultConfig {
        address: "http://127.0.0.1:1".to_string(),
        token: None,
        mount: "secret".to_string(),
        auth_method: "foobar_auth".to_string(),
        approle: None,
        kubernetes: None,
        aws_iam: None,
        jwt: None,
    };

    let result = VaultBackend::from_config(&config).await;
    assert!(result.is_err());
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("foobar_auth"),
        "error should name the unknown method: {msg}"
    );
    assert!(
        msg.contains("approle") || msg.contains("Supported"),
        "error should list supported methods: {msg}"
    );
}
