use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_backend")]
    pub backend: String,

    #[serde(default)]
    pub vault: Option<VaultConfig>,

    #[serde(default)]
    pub aws: Option<AwsConfig>,

    #[serde(default)]
    pub file: Option<FileConfig>,

    #[serde(default)]
    pub rotation: RotationConfig,

    /// Legacy database config (deprecated, use targets.postgres instead)
    #[serde(default)]
    pub database: Option<PostgresTargetConfig>,

    /// Target configurations for password updates
    #[serde(default)]
    pub targets: Option<TargetsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub address: String,
    pub token: String,
    #[serde(default = "default_mount")]
    pub mount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsConfig {
    #[serde(default = "default_aws_region")]
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConfig {
    /// Base directory for storing secret files
    /// Default: ~/.asr/secrets
    #[serde(default = "default_file_dir")]
    pub directory: String,
}

fn default_file_dir() -> String {
    format!(
        "{}/.asr/secrets",
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetsConfig {
    /// PostgreSQL target configuration
    #[serde(default)]
    pub postgres: Option<PostgresTargetConfig>,

    /// API target configuration
    #[serde(default)]
    pub api: Option<ApiTargetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresTargetConfig {
    pub host: String,
    #[serde(default = "default_db_port")]
    pub port: u16,
    pub database: String,
    pub username: String,
    /// Path in secret backend for admin password (optional if password provided directly)
    #[serde(default)]
    pub password_path: Option<String>,
    /// Direct password (not recommended, use password_path instead)
    #[serde(default)]
    pub password: Option<String>,
    /// SSL mode: disable, allow, prefer, require, verify-ca, verify-full
    #[serde(default = "default_ssl_mode")]
    pub ssl_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTargetConfig {
    /// Base URL for the API (e.g., "https://api.example.com")
    pub base_url: String,

    /// Endpoint path for password updates (e.g., "/api/v1/users/{username}/password")
    /// Use {username} as a placeholder that will be replaced
    pub endpoint: String,

    /// HTTP method (default: POST)
    #[serde(default = "default_api_method")]
    pub method: String,

    /// Field name in request body for password (default: "password")
    #[serde(default = "default_password_field")]
    pub password_field: String,

    /// Field name in request body for username (optional, username will be added if set)
    #[serde(default)]
    pub username_field: Option<String>,

    /// Additional fields to include in request body
    #[serde(default)]
    pub additional_fields: Option<std::collections::HashMap<String, String>>,

    /// Authorization header value (e.g., "Bearer token123")
    #[serde(default)]
    pub auth_header: Option<String>,

    /// Additional HTTP headers
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,

    /// Request timeout in seconds (default: 30)
    #[serde(default = "default_api_timeout")]
    pub timeout_seconds: u64,
}

fn default_api_method() -> String {
    "POST".to_string()
}

fn default_password_field() -> String {
    "password".to_string()
}

fn default_api_timeout() -> u64 {
    30
}

fn default_db_port() -> u16 {
    5432
}

fn default_ssl_mode() -> String {
    "prefer".to_string()
}

fn default_backend() -> String {
    "vault".to_string()
}

fn default_mount() -> String {
    "secret".to_string()
}

fn default_aws_region() -> String {
    "us-east-1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    #[serde(default = "default_rotation_period")]
    pub period_months: u32,
    #[serde(default = "default_secret_length")]
    pub secret_length: usize,
}

fn default_rotation_period() -> u32 {
    6
}

fn default_secret_length() -> usize {
    32
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            period_months: default_rotation_period(),
            secret_length: default_secret_length(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;

        toml::from_str(&contents).context("Failed to parse config file")
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let backend = std::env::var("SECRET_BACKEND")
            .unwrap_or_else(|_| "vault".to_string())
            .to_lowercase();

        let vault = if backend == "vault" {
            Some(VaultConfig {
                address: std::env::var("VAULT_ADDR")
                    .context("VAULT_ADDR environment variable not set")?,
                token: std::env::var("VAULT_TOKEN")
                    .context("VAULT_TOKEN environment variable not set")?,
                mount: std::env::var("VAULT_MOUNT").unwrap_or_else(|_| "secret".to_string()),
            })
        } else {
            None
        };

        let aws = if backend == "aws" {
            Some(AwsConfig {
                region: std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            })
        } else {
            None
        };

        let file = if backend == "file" {
            Some(FileConfig {
                directory: std::env::var("ASR_FILE_DIR").unwrap_or_else(|_| default_file_dir()),
            })
        } else {
            None
        };

        let rotation = RotationConfig {
            period_months: std::env::var("ROTATION_PERIOD_MONTHS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(6),
            secret_length: std::env::var("SECRET_LENGTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(32),
        };

        let database = if std::env::var("DB_HOST").is_ok() {
            Some(PostgresTargetConfig {
                host: std::env::var("DB_HOST").context("DB_HOST environment variable not set")?,
                port: std::env::var("DB_PORT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5432),
                database: std::env::var("DB_NAME").unwrap_or_else(|_| "postgres".to_string()),
                username: std::env::var("DB_USERNAME")
                    .context("DB_USERNAME environment variable not set")?,
                password_path: std::env::var("DB_PASSWORD_PATH").ok(),
                password: std::env::var("DB_PASSWORD").ok(),
                ssl_mode: std::env::var("DB_SSL_MODE").unwrap_or_else(|_| "prefer".to_string()),
            })
        } else {
            None
        };

        Ok(Self {
            backend,
            vault,
            aws,
            file,
            rotation,
            database,
            targets: None,
        })
    }

    /// Create a sample configuration file
    pub fn create_sample<P: AsRef<Path>>(path: P) -> Result<()> {
        let sample = Self {
            backend: "vault".to_string(),
            vault: Some(VaultConfig {
                address: "http://127.0.0.1:8200".to_string(),
                token: "your-vault-token-here".to_string(),
                mount: "secret".to_string(),
            }),
            aws: Some(AwsConfig {
                region: "us-east-1".to_string(),
            }),
            file: Some(FileConfig {
                directory: default_file_dir(),
            }),
            rotation: RotationConfig::default(),
            database: None,
            targets: None,
        };

        let toml_string =
            toml::to_string_pretty(&sample).context("Failed to serialize sample config")?;
        fs::write(path.as_ref(), toml_string)
            .with_context(|| format!("Failed to write sample config to {:?}", path.as_ref()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let postgres = config.targets.as_ref().unwrap().postgres.as_ref().unwrap();
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
        let api = config.targets.as_ref().unwrap().api.as_ref().unwrap();
        assert_eq!(api.base_url, "https://api.example.com");
        assert_eq!(api.endpoint, "/users/{username}/password");
        assert_eq!(api.method, "PUT");
        assert_eq!(api.password_field, "new_password");
        assert_eq!(api.username_field.as_ref().unwrap(), "user");
        assert_eq!(api.timeout_seconds, 60);
        assert_eq!(api.auth_header.as_ref().unwrap(), "Bearer token123");
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
        // Test defaults
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
        assert_eq!(config.backend, "vault");
        assert!(config.vault.is_some());
        assert!(config.aws.is_some());
        assert!(config.file.is_some());
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
        let postgres = config.targets.as_ref().unwrap().postgres.as_ref().unwrap();
        assert_eq!(postgres.port, 5432); // default port
        assert_eq!(postgres.ssl_mode, "prefer"); // default ssl_mode
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
        let api = config.targets.as_ref().unwrap().api.as_ref().unwrap();
        assert_eq!(api.method, "POST"); // default method
        assert_eq!(api.password_field, "password"); // default password_field
        assert_eq!(api.timeout_seconds, 30); // default timeout
    }
}
