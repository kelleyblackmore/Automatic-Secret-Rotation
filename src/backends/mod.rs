//! Secret backend implementations
//!
//! This module provides abstractions and implementations for different secret backends.
//! Available backends:
//! - **vault**: HashiCorp Vault KV v2 (always compiled)
//! - **aws**: AWS Secrets Manager (always compiled)
//! - **file**: Local file storage for development/testing (always compiled)
//! - **azure**: Azure Key Vault (requires `--features azure`)
//! - **gcp**: GCP Secret Manager (requires `--features gcp`)
//! - **ocp**: OpenShift / Kubernetes Secrets (requires `--features ocp`)

mod aws_secrets;
mod file;
mod secret_backend;
mod vault;

#[cfg(feature = "azure")]
pub mod azure_keyvault;
#[cfg(feature = "gcp")]
pub mod gcp_secret_manager;
#[cfg(feature = "ocp")]
pub mod ocp;

#[allow(unused_imports)]
pub use aws_secrets::{create_test_client, AwsSecretsClient};
pub use file::FileBackend;
pub use secret_backend::{SecretBackend, SecretData};
#[allow(unused_imports)]
pub use vault::{SecretMetadata, VaultBackend, VaultClient, VaultSecretData, VaultWriteRequest};

#[cfg(feature = "azure")]
pub use azure_keyvault::AzureKeyVaultBackend;
#[cfg(feature = "gcp")]
pub use gcp_secret_manager::GcpSecretManagerBackend;
#[cfg(feature = "ocp")]
pub use ocp::OcpBackend;

/// Backend type enumeration for type-safe backend selection by library consumers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BackendType {
    Vault,
    Aws,
    File,
    Azure,
    Gcp,
    Ocp,
}

impl std::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vault" => Ok(BackendType::Vault),
            "aws" => Ok(BackendType::Aws),
            "file" => Ok(BackendType::File),
            "azure" => Ok(BackendType::Azure),
            "gcp" => Ok(BackendType::Gcp),
            "ocp" | "k8s" | "kubernetes" => Ok(BackendType::Ocp),
            _ => Err(format!(
                "Unknown backend type: {}. Supported: vault, aws, file, azure, gcp, ocp",
                s
            )),
        }
    }
}

/// Type alias for backend trait object
pub type Backend = Box<dyn SecretBackend>;
