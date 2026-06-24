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
    pub azure: Option<AzureConfig>,

    #[serde(default)]
    pub gcp: Option<GcpConfig>,

    #[serde(default)]
    pub ocp: Option<OcpConfig>,

    #[serde(default)]
    pub rotation: RotationConfig,

    /// Legacy database config (deprecated, use targets.postgres instead)
    #[serde(default)]
    pub database: Option<PostgresTargetConfig>,

    /// Target configuration: either `[targets.postgres]`/`[targets.api]` (old)
    /// or `[[targets]]` array with `type` field (new multi-target form).
    #[serde(default)]
    pub targets: Option<TargetsSpec>,

    /// Optional audit log configuration
    #[serde(default)]
    pub audit: AuditConfig,

    /// Optional webhook/Slack notification configuration
    #[serde(default)]
    pub notification: NotificationConfig,
}

// ---------------------------------------------------------------------------
// Backend configs
// ---------------------------------------------------------------------------

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
    /// Base directory for storing secret files. Default: ~/.asr/secrets
    #[serde(default = "default_file_dir")]
    pub directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzureConfig {
    /// Azure Key Vault URL, e.g. "https://my-vault.vault.azure.net"
    pub vault_url: String,
    /// Optional tenant ID (defaults to DefaultAzureCredential discovery)
    #[serde(default)]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcpConfig {
    /// GCP project ID
    pub project_id: String,
    /// Optional path to service account key JSON (defaults to ADC)
    #[serde(default)]
    pub credentials_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcpConfig {
    /// Kubernetes namespace to read/write secrets in
    pub namespace: String,
    /// Optional path to kubeconfig (defaults to in-cluster auth, then ~/.kube/config)
    #[serde(default)]
    pub kubeconfig: Option<String>,
    /// Optional kubeconfig context name
    #[serde(default)]
    pub context: Option<String>,
}

// ---------------------------------------------------------------------------
// Target configs
// ---------------------------------------------------------------------------

/// Supports both the old named form (`[targets.postgres]`) and the new array
/// form (`[[targets]]` with a `type` field).  serde's untagged enum tries
/// each variant in order: Vec first (array), then TargetsConfig (named table).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum TargetsSpec {
    /// New: `[[targets]]` with `type = "postgres"` / `"api"` / `"mysql"` / `"gitlab"` / `"github"`
    List(Vec<TargetEntry>),
    /// Old: `[targets.postgres]` / `[targets.api]` (backward compat)
    Named(TargetsConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetsConfig {
    #[serde(default)]
    pub postgres: Option<PostgresTargetConfig>,
    #[serde(default)]
    pub api: Option<ApiTargetConfig>,
    #[serde(default)]
    pub mysql: Option<MysqlTargetConfig>,
    #[serde(default)]
    pub gitlab: Option<GitLabTargetConfig>,
    #[serde(default)]
    pub github: Option<GitHubTargetConfig>,
}

/// One entry in the `[[targets]]` array form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TargetEntry {
    Postgres(PostgresTargetConfig),
    Api(ApiTargetConfig),
    Mysql(MysqlTargetConfig),
    Gitlab(GitLabTargetConfig),
    Github(GitHubTargetConfig),
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
pub struct MysqlTargetConfig {
    pub host: String,
    #[serde(default = "default_mysql_port")]
    pub port: u16,
    pub database: String,
    pub username: String,
    #[serde(default)]
    pub password_path: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub ssl_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTargetConfig {
    /// Base URL for the API (e.g., "https://api.example.com")
    pub base_url: String,

    /// Endpoint path for password updates (e.g., "/api/v1/users/{username}/password")
    pub endpoint: String,

    /// HTTP method (default: POST)
    #[serde(default = "default_api_method")]
    pub method: String,

    /// Field name in request body for password (default: "password")
    #[serde(default = "default_password_field")]
    pub password_field: String,

    /// Field name in request body for username (optional)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabTargetConfig {
    /// GitLab project ID (numeric) or path ("group/project" — slashes are auto-encoded)
    pub project_id: String,
    /// CI/CD variable key to create or update
    pub variable_key: String,
    /// GitLab instance base URL (defaults to https://gitlab.com)
    #[serde(default)]
    pub gitlab_url: Option<String>,
    /// Personal/project/group access token (falls back to GITLAB_TOKEN env var).
    /// Use `token_path` to source the token from the secret backend instead.
    #[serde(default)]
    pub token: Option<String>,
    /// Path in the secret backend where the GitLab token is stored.
    /// Takes precedence over `token` and `GITLAB_TOKEN`.
    #[serde(default)]
    pub token_path: Option<String>,
    /// Mark the variable as masked in job logs
    #[serde(default)]
    pub masked: Option<bool>,
    /// Restrict the variable to protected branches/tags only
    #[serde(default)]
    pub protected: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubTargetConfig {
    /// Repository owner (organization or user)
    pub owner: String,
    /// Repository name
    pub repo: String,
    /// Name of the Actions secret to update (mutually exclusive with variable_name)
    #[serde(default)]
    pub secret_name: Option<String>,
    /// Name of the Actions variable to update (mutually exclusive with secret_name)
    #[serde(default)]
    pub variable_name: Option<String>,
    /// Personal access token (falls back to GITHUB_TOKEN env var).
    /// Use `token_path` to source the token from the secret backend instead.
    #[serde(default)]
    pub token: Option<String>,
    /// Path in the secret backend where the GitHub token is stored.
    /// Takes precedence over `token` and `GITHUB_TOKEN`.
    #[serde(default)]
    pub token_path: Option<String>,
    /// GitHub Environment name to scope the secret/variable (optional)
    #[serde(default)]
    pub env_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Audit log config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditConfig {
    /// Path to JSONL append-only audit log file
    #[serde(default)]
    pub log_file: Option<String>,
    /// Also write audit events to stdout as structured JSON
    #[serde(default)]
    pub stdout: bool,
}

// ---------------------------------------------------------------------------
// Notification config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    /// Webhook URL for rotation event POST notifications
    #[serde(default)]
    pub webhook_url: Option<String>,
    /// Optional Authorization header value (e.g. "Bearer token123")
    #[serde(default)]
    pub auth_header: Option<String>,
    /// Events to notify on (default: all). Options: rotate, flag, scan
    #[serde(default = "default_notification_events")]
    pub events: Vec<String>,
}

fn default_notification_events() -> Vec<String> {
    vec!["rotate".to_string(), "flag".to_string(), "scan".to_string()]
}

// ---------------------------------------------------------------------------
// Rotation config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    #[serde(default = "default_rotation_period")]
    pub period_months: u32,
    #[serde(default = "default_secret_length")]
    pub secret_length: usize,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            period_months: default_rotation_period(),
            secret_length: default_secret_length(),
        }
    }
}

// ---------------------------------------------------------------------------
// Default value functions
// ---------------------------------------------------------------------------

fn default_file_dir() -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    format!("{}/.asr/secrets", home)
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

fn default_mysql_port() -> u16 {
    3306
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

fn default_rotation_period() -> u32 {
    6
}

fn default_secret_length() -> usize {
    32
}

// ---------------------------------------------------------------------------
// Config impl
// ---------------------------------------------------------------------------

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;
        toml::from_str(&contents).context("Failed to parse config file")
    }

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

        let azure = if backend == "azure" {
            Some(AzureConfig {
                vault_url: std::env::var("AZURE_VAULT_URL")
                    .context("AZURE_VAULT_URL environment variable not set")?,
                tenant_id: std::env::var("AZURE_TENANT_ID").ok(),
            })
        } else {
            None
        };

        let gcp = if backend == "gcp" {
            Some(GcpConfig {
                project_id: std::env::var("GCP_PROJECT_ID")
                    .context("GCP_PROJECT_ID environment variable not set")?,
                credentials_file: std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok(),
            })
        } else {
            None
        };

        let ocp = if backend == "ocp" {
            Some(OcpConfig {
                namespace: std::env::var("OCP_NAMESPACE").unwrap_or_else(|_| "default".to_string()),
                kubeconfig: std::env::var("KUBECONFIG").ok(),
                context: None,
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

        let audit = AuditConfig {
            log_file: std::env::var("ASR_AUDIT_LOG").ok(),
            stdout: std::env::var("ASR_AUDIT_STDOUT")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        };

        let notification = NotificationConfig {
            webhook_url: std::env::var("ASR_WEBHOOK_URL").ok(),
            auth_header: std::env::var("ASR_WEBHOOK_AUTH").ok(),
            events: default_notification_events(),
        };

        Ok(Self {
            backend,
            vault,
            aws,
            file,
            azure,
            gcp,
            ocp,
            rotation,
            database,
            targets: None,
            audit,
            notification,
        })
    }

    pub fn create_sample<P: AsRef<Path>>(path: P) -> Result<()> {
        let sample = r#"# Automatic Secret Rotation configuration
# Set the backend to use: vault, aws, file, azure, gcp, ocp
backend = "vault"

[vault]
address = "http://127.0.0.1:8200"
token = "your-vault-token-here"
mount = "secret"

# [aws]
# region = "us-east-1"

# [file]
# directory = "~/.asr/secrets"

# [azure]
# vault_url = "https://my-vault.vault.azure.net"

# [gcp]
# project_id = "my-gcp-project"

# [ocp]
# namespace = "my-app"

[rotation]
period_months = 6
secret_length = 32

# Single PostgreSQL target (old form)
# [targets.postgres]
# host = "localhost"
# port = 5432
# database = "postgres"
# username = "admin"
# password_path = "admin/password"

# Multiple targets (new form)
# [[targets]]
# type = "postgres"
# host = "primary.db.internal"
# database = "app"
# username = "admin"
# password_path = "myapp/db-admin-password"
#
# [[targets]]
# type = "postgres"
# host = "replica.db.internal"
# database = "app"
# username = "admin"
# password_path = "myapp/db-admin-password"

# GitLab CI/CD variable target (requires --features gitlab)
# [[targets]]
# type = "gitlab"
# project_id = "mygroup/myproject"   # or numeric ID
# variable_key = "DB_PASSWORD"
# # gitlab_url = "https://gitlab.example.com"   # defaults to https://gitlab.com
# # token = "glpat-..."                          # or set GITLAB_TOKEN env var
# # masked = true
# # protected = false

# GitHub Actions secret target (requires --features github)
# [[targets]]
# type = "github"
# owner = "myorg"
# repo  = "myrepo"
# secret_name = "DB_PASSWORD"          # mutually exclusive with variable_name
# # variable_name = "DB_HOST"          # use this for plaintext variables instead
# # token = "ghp_..."                  # or set GITHUB_TOKEN env var
# # env_name = "production"            # scope to a GitHub Environment

# [audit]
# log_file = "/var/log/asr/audit.jsonl"
# stdout = false

# [notification]
# webhook_url = "https://hooks.slack.com/services/..."
# auth_header = "Bearer token123"
# events = ["rotate", "flag", "scan"]
"#;

        fs::write(path.as_ref(), sample)
            .with_context(|| format!("Failed to write sample config to {:?}", path.as_ref()))?;

        Ok(())
    }
}
