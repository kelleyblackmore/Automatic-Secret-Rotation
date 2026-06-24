use anyhow::Result;
use secret_rotator::backends::{FileBackend, SecretBackend};
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::test]
async fn test_write_and_read_secret() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;

    let mut data = HashMap::new();
    data.insert("password".to_string(), "test123".to_string());
    data.insert("username".to_string(), "admin".to_string());

    backend.write_secret("test/secret", data.clone()).await?;

    let secret = backend.read_secret("test/secret").await?;
    assert_eq!(secret.data, data);

    Ok(())
}

#[tokio::test]
async fn test_metadata() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;

    let mut metadata = HashMap::new();
    metadata.insert("rotation_enabled".to_string(), "true".to_string());
    metadata.insert("last_rotated".to_string(), "2024-01-01".to_string());

    backend
        .update_metadata("test/secret", metadata.clone())
        .await?;

    let read_meta = backend.read_metadata("test/secret").await?;
    assert_eq!(read_meta, metadata);

    Ok(())
}

#[tokio::test]
async fn test_list_secrets() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;

    let mut data1 = HashMap::new();
    data1.insert("password".to_string(), "pass1".to_string());
    backend.write_secret("app/db", data1).await?;

    let mut data2 = HashMap::new();
    data2.insert("token".to_string(), "token1".to_string());
    backend.write_secret("app/api", data2).await?;

    let secrets = backend.list_secrets("").await?;
    assert!(secrets.contains(&"app/db".to_string()));
    assert!(secrets.contains(&"app/api".to_string()));

    Ok(())
}

#[test]
fn test_parse_line() {
    assert_eq!(
        FileBackend::parse_line("password:test123"),
        Some(("password".to_string(), "test123".to_string()))
    );
    assert_eq!(
        FileBackend::parse_line("  key  :  value  "),
        Some(("key".to_string(), "value".to_string()))
    );
    assert_eq!(FileBackend::parse_line("# comment"), None);
    assert_eq!(FileBackend::parse_line(""), None);
}
