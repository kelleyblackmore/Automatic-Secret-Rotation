#![cfg(feature = "gitlab")]

use anyhow::Result;
use secret_rotator::config::GitLabTargetConfig;
use secret_rotator::targets::{GitLabTarget, Target};

fn gitlab_config(server_url: &str, project_id: &str, var_key: &str) -> GitLabTargetConfig {
    GitLabTargetConfig {
        project_id: project_id.to_string(),
        variable_key: var_key.to_string(),
        gitlab_url: Some(server_url.to_string()),
        token: Some("glpat-test-token".to_string()),
        token_path: None,
        masked: None,
        protected: None,
    }
}

// ---------------------------------------------------------------------------
// Target trait properties
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gitlab_target_does_not_require_username() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/v4/projects/1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":1,"name":"test"}"#)
        .create_async()
        .await;

    let target = GitLabTarget::new(&gitlab_config(&server.url(), "1", "MY_VAR")).await?;
    assert!(!target.requires_username());
    assert_eq!(target.target_type(), "GitLab");
    Ok(())
}

// ---------------------------------------------------------------------------
// update_password — update an existing variable (PUT → 200)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gitlab_update_existing_variable_sends_put() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let put_mock = server
        .mock("PUT", "/api/v4/projects/123/variables/DB_PASSWORD")
        .match_header("PRIVATE-TOKEN", "glpat-test-token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"key":"DB_PASSWORD","value":"newpass"}"#)
        .create_async()
        .await;

    let config = GitLabTargetConfig {
        project_id: "123".to_string(),
        variable_key: "DB_PASSWORD".to_string(),
        gitlab_url: Some(server.url()),
        token: Some("glpat-test-token".to_string()),
        token_path: None,
        masked: Some(false),
        protected: Some(false),
    };
    let target = GitLabTarget::new(&config).await?;
    target.update_password("", "newpass").await?;

    put_mock.assert_async().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// update_password — variable absent: PUT → 404 then POST → 201
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gitlab_creates_variable_when_put_returns_404() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let put_mock = server
        .mock("PUT", "/api/v4/projects/42/variables/NEW_VAR")
        .with_status(404)
        .with_body(r#"{"message":"Not Found"}"#)
        .create_async()
        .await;

    let post_mock = server
        .mock("POST", "/api/v4/projects/42/variables")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"key":"NEW_VAR","value":"newpass"}"#)
        .create_async()
        .await;

    let target = GitLabTarget::new(&gitlab_config(&server.url(), "42", "NEW_VAR")).await?;
    target.update_password("", "newpass").await?;

    put_mock.assert_async().await;
    post_mock.assert_async().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// update_password — server error propagates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gitlab_update_propagates_server_error() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let _put = server
        .mock("PUT", "/api/v4/projects/1/variables/MY_VAR")
        .with_status(500)
        .with_body(r#"{"message":"Internal Server Error"}"#)
        .create_async()
        .await;

    let target = GitLabTarget::new(&gitlab_config(&server.url(), "1", "MY_VAR")).await?;
    let result = target.update_password("", "pass").await;
    assert!(result.is_err(), "server error should be propagated");
    Ok(())
}

// ---------------------------------------------------------------------------
// update_password — slash in project ID is percent-encoded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gitlab_project_path_slash_is_encoded() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    // "mygroup/myproject" should appear as "mygroup%2Fmyproject" in the URL
    let put_mock = server
        .mock("PUT", "/api/v4/projects/mygroup%2Fmyproject/variables/MY_VAR")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"key":"MY_VAR","value":"v"}"#)
        .create_async()
        .await;

    let target =
        GitLabTarget::new(&gitlab_config(&server.url(), "mygroup/myproject", "MY_VAR")).await?;
    target.update_password("", "v").await?;

    put_mock.assert_async().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// verify_connection — GET project succeeds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gitlab_verify_connection_get_project() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let get_mock = server
        .mock("GET", "/api/v4/projects/99")
        .match_header("PRIVATE-TOKEN", "glpat-test-token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":99,"name":"test-project"}"#)
        .create_async()
        .await;

    let target = GitLabTarget::new(&gitlab_config(&server.url(), "99", "MY_VAR")).await?;
    target.verify_connection("", "", None).await?;

    get_mock.assert_async().await;
    Ok(())
}

#[tokio::test]
async fn test_gitlab_verify_connection_failure_propagates_error() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    let _get = server
        .mock("GET", "/api/v4/projects/1")
        .with_status(401)
        .with_body(r#"{"message":"401 Unauthorized"}"#)
        .create_async()
        .await;

    let target = GitLabTarget::new(&gitlab_config(&server.url(), "1", "MY_VAR")).await?;
    let result = target.verify_connection("", "", None).await;
    assert!(result.is_err(), "401 should propagate as an error");
    Ok(())
}

// ---------------------------------------------------------------------------
// masked / protected flags are passed in create body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gitlab_create_body_includes_masked_flag() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    // PUT → 404 forces creation
    let _put = server
        .mock("PUT", "/api/v4/projects/1/variables/SECRET_VAR")
        .with_status(404)
        .create_async()
        .await;

    let post_mock = server
        .mock("POST", "/api/v4/projects/1/variables")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "key": "SECRET_VAR",
            "masked": true,
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"key":"SECRET_VAR","value":"s"}"#)
        .create_async()
        .await;

    let config = GitLabTargetConfig {
        project_id: "1".to_string(),
        variable_key: "SECRET_VAR".to_string(),
        gitlab_url: Some(server.url()),
        token: Some("tok".to_string()),
        token_path: None,
        masked: Some(true),
        protected: Some(false),
    };
    let target = GitLabTarget::new(&config).await?;
    target.update_password("", "s").await?;

    post_mock.assert_async().await;
    Ok(())
}
