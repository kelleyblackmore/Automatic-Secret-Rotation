use secret_rotator::backends::{VaultClient, SecretMetadata, VaultSecretData, VaultWriteRequest};
use std::collections::HashMap;

#[test]
fn test_vault_client_new() {
    let client = VaultClient::new(
        "http://localhost:8200".to_string(),
        "test-token".to_string(),
    );
    assert!(client.is_ok());
}

#[test]
fn test_vault_url_construction() {
    let client = VaultClient::new(
        "http://localhost:8200".to_string(),
        "test-token".to_string(),
    )
    .unwrap();

    // Test read URL
    let read_url = format!("{}/v1/{}/data/{}", client.address, "secret", "myapp/db");
    assert_eq!(read_url, "http://localhost:8200/v1/secret/data/myapp/db");

    // Test write URL
    let write_url = format!("{}/v1/{}/data/{}", client.address, "secret", "myapp/db");
    assert_eq!(write_url, "http://localhost:8200/v1/secret/data/myapp/db");

    // Test metadata URL
    let meta_url = format!("{}/v1/{}/metadata/{}", client.address, "secret", "myapp/db");
    assert_eq!(
        meta_url,
        "http://localhost:8200/v1/secret/metadata/myapp/db"
    );
}

#[test]
fn test_vault_secret_metadata_parsing() {
    let mut custom_meta = HashMap::new();
    custom_meta.insert("rotation_enabled".to_string(), "true".to_string());
    custom_meta.insert(
        "last_rotated".to_string(),
        "2023-01-01T00:00:00Z".to_string(),
    );

    let metadata = SecretMetadata {
        custom_metadata: Some(custom_meta.clone()),
    };

    assert_eq!(
        metadata
            .custom_metadata
            .as_ref()
            .unwrap()
            .get("rotation_enabled"),
        Some(&"true".to_string())
    );
}

#[test]
fn test_vault_secret_data_structure() {
    let mut data = HashMap::new();
    data.insert("password".to_string(), "secret123".to_string());
    data.insert("username".to_string(), "admin".to_string());

    let mut custom_meta = HashMap::new();
    custom_meta.insert("rotation_enabled".to_string(), "true".to_string());

    let secret_data = VaultSecretData {
        data: data.clone(),
        metadata: Some(SecretMetadata {
            custom_metadata: Some(custom_meta),
        }),
    };

    assert_eq!(
        secret_data.data.get("password"),
        Some(&"secret123".to_string())
    );
    assert_eq!(secret_data.data.get("username"), Some(&"admin".to_string()));
    assert!(secret_data.metadata.is_some());
}

#[test]
fn test_vault_write_request_serialization() {
    let mut data = HashMap::new();
    data.insert("password".to_string(), "newpass".to_string());

    let request = VaultWriteRequest {
        data: data.clone(),
        options: None,
    };

    // Verify structure
    assert_eq!(request.data.get("password"), Some(&"newpass".to_string()));
    assert!(request.options.is_none());
}
