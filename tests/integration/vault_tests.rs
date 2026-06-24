//! Integration tests against a live HashiCorp Vault dev instance.
//!
//! Requires the Docker Compose stack to be running:
//!   docker compose -f tests/integration/docker-compose.yml up -d
//!
//! Run with:
//!   cargo test --features integration -- --ignored vault

#![cfg(feature = "integration")]

use std::collections::HashMap;

use anyhow::Result;
use secret_rotator::backends::{SecretBackend, VaultBackend, VaultClient};
use secret_rotator::rotation;

const VAULT_ADDR: &str = "http://127.0.0.1:8200";
const VAULT_TOKEN: &str = "test-root-token";
const VAULT_MOUNT: &str = "secret";

fn vault_backend() -> impl SecretBackend {
    let client = VaultClient::new(VAULT_ADDR.to_string(), VAULT_TOKEN.to_string())
        .expect("Failed to create VaultClient");
    VaultBackend::new(client, VAULT_MOUNT.to_string())
}

#[tokio::test]
#[ignore = "requires Vault docker-compose stack"]
async fn test_vault_write_and_read() -> Result<()> {
    let backend = vault_backend();
    let path = "integration/test-write-read";

    let mut data = HashMap::new();
    data.insert("password".to_string(), "initial-secret".to_string());
    backend.write_secret(path, data).await?;

    let secret = backend.read_secret(path).await?;
    assert_eq!(secret.data.get("password").map(String::as_str), Some("initial-secret"));

    Ok(())
}

#[tokio::test]
#[ignore = "requires Vault docker-compose stack"]
async fn test_vault_rotate_updates_value() -> Result<()> {
    let backend = vault_backend();
    let path = "integration/test-rotate";

    // Write initial value and flag for rotation
    let mut data = HashMap::new();
    data.insert("password".to_string(), "old-password-abc".to_string());
    backend.write_secret(path, data).await?;
    rotation::flag_for_rotation(&backend, path, 0).await?; // period=0 means always due

    // Rotate
    let new_secret = rotation::rotate_secret(&backend, path, 32).await?;
    assert_ne!(new_secret, "old-password-abc", "New secret should differ from old");
    assert_eq!(new_secret.len(), 32);

    // Read back and verify
    let stored = backend.read_secret(path).await?;
    let stored_value = stored.data.get("password").expect("password key missing");
    assert_eq!(stored_value, &new_secret, "Stored value should match returned secret");

    Ok(())
}

#[tokio::test]
#[ignore = "requires Vault docker-compose stack"]
async fn test_vault_list_secrets() -> Result<()> {
    let backend = vault_backend();
    let base = "integration/list-test";

    // Write several secrets
    for name in &["alpha", "beta", "gamma"] {
        let path = format!("{}/{}", base, name);
        let mut data = HashMap::new();
        data.insert("key".to_string(), "value".to_string());
        backend.write_secret(&path, data).await?;
    }

    let secrets = backend.list_secrets(base).await?;
    assert!(
        secrets.len() >= 3,
        "Expected at least 3 secrets, got {}",
        secrets.len()
    );

    Ok(())
}

#[tokio::test]
#[ignore = "requires Vault docker-compose stack"]
async fn test_vault_metadata_round_trip() -> Result<()> {
    let backend = vault_backend();
    let path = "integration/meta-round-trip";

    let mut data = HashMap::new();
    data.insert("password".to_string(), "meta-test".to_string());
    backend.write_secret(path, data).await?;

    let mut meta = HashMap::new();
    meta.insert("rotation_enabled".to_string(), "true".to_string());
    meta.insert("rotation_period_months".to_string(), "3".to_string());
    backend.update_metadata(path, meta).await?;

    let read_meta = backend.read_metadata(path).await?;
    assert_eq!(
        read_meta.get("rotation_enabled").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        read_meta.get("rotation_period_months").map(String::as_str),
        Some("3")
    );

    Ok(())
}

#[tokio::test]
#[ignore = "requires Vault docker-compose stack"]
async fn test_vault_scan_returns_due_secrets() -> Result<()> {
    let backend = vault_backend();
    let path = "integration/scan-test/secret";

    let mut data = HashMap::new();
    data.insert("password".to_string(), "scan-me".to_string());
    backend.write_secret(path, data).await?;

    // Flag with period=0 (always due)
    rotation::flag_for_rotation(&backend, path, 0).await?;

    let due = rotation::scan_for_rotation(&backend, "integration/scan-test", 6).await?;
    assert!(
        due.iter().any(|s| s.contains("scan-test")),
        "Expected scan-test secret in due list, got: {:?}",
        due
    );

    Ok(())
}
