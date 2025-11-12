use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub vault: VaultConfig,
    #[serde(default)]
    pub rotation: RotationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub address: String,
    pub token: String,
    pub mount: String,
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
        let vault = VaultConfig {
            address: std::env::var("VAULT_ADDR")
                .context("VAULT_ADDR environment variable not set")?,
            token: std::env::var("VAULT_TOKEN")
                .context("VAULT_TOKEN environment variable not set")?,
            mount: std::env::var("VAULT_MOUNT").unwrap_or_else(|_| "secret".to_string()),
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

        Ok(Self { vault, rotation })
    }

    /// Create a sample configuration file
    pub fn create_sample<P: AsRef<Path>>(path: P) -> Result<()> {
        let sample = Self {
            vault: VaultConfig {
                address: "http://127.0.0.1:8200".to_string(),
                token: "your-vault-token-here".to_string(),
                mount: "secret".to_string(),
            },
            rotation: RotationConfig::default(),
        };

        let toml_string =
            toml::to_string_pretty(&sample).context("Failed to serialize sample config")?;
        fs::write(path.as_ref(), toml_string)
            .with_context(|| format!("Failed to write sample config to {:?}", path.as_ref()))?;

        Ok(())
    }
}
