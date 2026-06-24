use secret_rotator::config::{Config, RotationConfig, TargetsSpec};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_default_rotation_config() {
    let config = RotationConfig::default();
    assert_eq!(config.period_months, 6);
    assert_eq!(config.secret_length, 32);
}

#[test]
fn test_config_from_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
backend = "vault"
[vault]
address = "http://localhost:8200"
token = "test-token"
mount = "secret"

[rotation]
period_months = 12
secret_length = 64
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.backend, "vault");
    assert_eq!(
        config.vault.as_ref().unwrap().address,
        "http://localhost:8200"
    );
    assert_eq!(config.rotation.period_months, 12);
    assert_eq!(config.rotation.secret_length, 64);
}

#[test]
fn test_config_from_file_with_aws() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
backend = "aws"
[aws]
region = "us-west-2"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.backend, "aws");
    assert_eq!(config.aws.as_ref().unwrap().region, "us-west-2");
}

#[test]
fn test_config_from_file_with_file_backend() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
backend = "file"
[file]
directory = "/tmp/test-secrets"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.backend, "file");
    assert_eq!(config.file.as_ref().unwrap().directory, "/tmp/test-secrets");
}

/// Helper to extract TargetsConfig from the Named variant of TargetsSpec.
fn named_targets(config: &Config) -> &secret_rotator::config::TargetsConfig {
    match config.targets.as_ref().unwrap() {
        TargetsSpec::Named(named) => named,
        TargetsSpec::List(_) => panic!("Expected Named targets, got List"),
    }
}

#[test]
fn test_config_from_file_with_targets() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
backend = "vault"
[vault]
address = "http://localhost:8200"
token = "test-token"

[targets.postgres]
host = "localhost"
port = 5432
database = "testdb"
username = "admin"
password_path = "admin/password"
ssl_mode = "require"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert!(config.targets.is_some());
    let postgres = named_targets(&config).postgres.as_ref().unwrap();
    assert_eq!(postgres.host, "localhost");
    assert_eq!(postgres.port, 5432);
    assert_eq!(postgres.database, "testdb");
    assert_eq!(postgres.username, "admin");
    assert_eq!(postgres.password_path.as_ref().unwrap(), "admin/password");
    assert_eq!(postgres.ssl_mode, "require");
}

#[test]
fn test_config_from_file_with_api_target() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
backend = "vault"
[vault]
address = "http://localhost:8200"
token = "test-token"

[targets.api]
base_url = "https://api.example.com"
endpoint = "/users/{username}/password"
method = "PUT"
password_field = "new_password"
username_field = "user"
timeout_seconds = 60
auth_header = "Bearer token123"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    let api = named_targets(&config).api.as_ref().unwrap();
    assert_eq!(api.base_url, "https://api.example.com");
    assert_eq!(api.endpoint, "/users/{username}/password");
    assert_eq!(api.method, "PUT");
    assert_eq!(api.password_field, "new_password");
    assert_eq!(api.username_field.as_ref().unwrap(), "user");
    assert_eq!(api.timeout_seconds, 60);
    assert_eq!(api.auth_header.as_ref().unwrap(), "Bearer token123");
}

#[test]
fn test_config_from_file_multi_target_list() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
backend = "vault"
[vault]
address = "http://localhost:8200"
token = "test-token"

[[targets]]
type = "postgres"
host = "primary.db"
database = "app"
username = "admin"
password_path = "myapp/admin"

[[targets]]
type = "postgres"
host = "replica.db"
database = "app"
username = "admin"
password_path = "myapp/admin"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert!(config.targets.is_some());
    match config.targets.as_ref().unwrap() {
        TargetsSpec::List(entries) => {
            assert_eq!(entries.len(), 2, "Expected 2 targets");
        }
        TargetsSpec::Named(_) => panic!("Expected List targets, got Named"),
    }
}

#[test]
fn test_config_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
backend = "vault"
[vault]
address = "http://localhost:8200"
token = "test-token"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.vault.as_ref().unwrap().mount, "secret");
    assert_eq!(config.rotation.period_months, 6);
    assert_eq!(config.rotation.secret_length, 32);
}

#[test]
fn test_config_create_sample() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("sample.toml");

    Config::create_sample(&config_path).unwrap();

    assert!(config_path.exists());
    let config = Config::from_file(&config_path).unwrap();
    // Sample uses vault backend with required fields
    assert_eq!(config.backend, "vault");
    assert!(config.vault.is_some());
}

#[test]
fn test_postgres_config_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[targets.postgres]
host = "localhost"
database = "testdb"
username = "admin"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    let postgres = named_targets(&config).postgres.as_ref().unwrap();
    assert_eq!(postgres.port, 5432);
    assert_eq!(postgres.ssl_mode, "prefer");
}

#[test]
fn test_api_config_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[targets.api]
base_url = "https://api.example.com"
endpoint = "/password"
"#;
    fs::write(&config_path, config_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    let api = named_targets(&config).api.as_ref().unwrap();
    assert_eq!(api.method, "POST");
    assert_eq!(api.password_field, "password");
    assert_eq!(api.timeout_seconds, 30);
}
