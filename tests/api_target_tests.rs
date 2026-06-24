use secret_rotator::config::ApiTargetConfig;
use secret_rotator::targets::ApiTarget;

#[test]
fn test_build_url_with_placeholder() {
    let config = ApiTargetConfig {
        base_url: "https://api.example.com".to_string(),
        endpoint: "/users/{username}/password".to_string(),
        method: "POST".to_string(),
        password_field: "password".to_string(),
        username_field: Some("username".to_string()),
        additional_fields: None,
        auth_header: None,
        headers: None,
        timeout_seconds: 30,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let target = rt.block_on(ApiTarget::new(&config)).unwrap();

    let url = target.build_url("testuser");
    assert_eq!(url, "https://api.example.com/users/testuser/password");
}

#[test]
fn test_build_url_with_full_url() {
    let config = ApiTargetConfig {
        base_url: "https://api.example.com".to_string(),
        endpoint: "https://other.com/api/password".to_string(),
        method: "POST".to_string(),
        password_field: "password".to_string(),
        username_field: None,
        additional_fields: None,
        auth_header: None,
        headers: None,
        timeout_seconds: 30,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let target = rt.block_on(ApiTarget::new(&config)).unwrap();

    let url = target.build_url("testuser");
    assert_eq!(url, "https://other.com/api/password");
}

#[test]
fn test_build_url_with_relative_path() {
    let config = ApiTargetConfig {
        base_url: "https://api.example.com".to_string(),
        endpoint: "password".to_string(),
        method: "POST".to_string(),
        password_field: "password".to_string(),
        username_field: None,
        additional_fields: None,
        auth_header: None,
        headers: None,
        timeout_seconds: 30,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let target = rt.block_on(ApiTarget::new(&config)).unwrap();

    let url = target.build_url("testuser");
    assert_eq!(url, "https://api.example.com/password");
}

#[test]
fn test_build_url_with_trailing_slash() {
    let config = ApiTargetConfig {
        base_url: "https://api.example.com/".to_string(),
        endpoint: "/password".to_string(),
        method: "POST".to_string(),
        password_field: "password".to_string(),
        username_field: None,
        additional_fields: None,
        auth_header: None,
        headers: None,
        timeout_seconds: 30,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let target = rt.block_on(ApiTarget::new(&config)).unwrap();

    let url = target.build_url("testuser");
    assert_eq!(url, "https://api.example.com/password");
}
