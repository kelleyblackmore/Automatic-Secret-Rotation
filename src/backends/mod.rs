//! Secret backend implementations
//!
//! This module provides abstractions and implementations for different secret backends.

mod aws_secrets;
mod file;
mod secret_backend;
mod vault;

pub use aws_secrets::AwsSecretsClient;
pub use file::FileBackend;
pub use secret_backend::SecretBackend;
pub use vault::{VaultBackend, VaultClient};

/// Backend type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Reserved for future use (e.g., type-safe backend selection)
pub enum BackendType {
    Vault,
    Aws,
    File,
}

impl std::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vault" => Ok(BackendType::Vault),
            "aws" => Ok(BackendType::Aws),
            "file" => Ok(BackendType::File),
            _ => Err(format!(
                "Unknown backend type: {}. Supported: vault, aws, file",
                s
            )),
        }
    }
}

/// Type alias for backend trait object
pub type Backend = Box<dyn SecretBackend>;
