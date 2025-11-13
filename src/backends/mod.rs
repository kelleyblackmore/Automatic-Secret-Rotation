//! Secret backend implementations
//!
//! This module provides abstractions and implementations for different secret backends.

mod aws_secrets;
mod secret_backend;
mod vault;

pub use aws_secrets::AwsSecretsClient;
pub use secret_backend::SecretBackend;
pub use vault::{VaultBackend, VaultClient};

/// Backend type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Reserved for future use (e.g., type-safe backend selection)
pub enum BackendType {
    Vault,
    Aws,
}

impl std::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vault" => Ok(BackendType::Vault),
            "aws" => Ok(BackendType::Aws),
            _ => Err(format!("Unknown backend type: {}. Supported: vault, aws", s)),
        }
    }
}

/// Type alias for backend trait object
pub type Backend = Box<dyn SecretBackend>;

