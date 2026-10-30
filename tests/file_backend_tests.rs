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

fn one(key: &str, value: &str) -> HashMap<String, String> {
    HashMap::from([(key.to_string(), value.to_string())])
}

#[tokio::test]
async fn test_paths_outside_the_base_dir_are_rejected() -> Result<()> {
    let root = TempDir::new()?;
    let base = root.path().join("secrets");
    let backend = FileBackend::new(&base)?;
    let outside = root.path().join("outside");
    std::fs::write(&outside, "password:keep-me\n")?;

    for path in [
        "../outside",
        "app/../../outside",
        outside.to_str().unwrap(), // absolute
        "/etc/passwd",
    ] {
        assert!(backend.read_secret(path).await.is_err(), "read {path}");
        assert!(
            backend
                .write_secret(path, one("password", "x"))
                .await
                .is_err(),
            "write {path}"
        );
        assert!(
            backend.update_metadata(path, one("k", "v")).await.is_err(),
            "metadata {path}"
        );
    }
    assert_eq!(std::fs::read_to_string(&outside)?, "password:keep-me\n");
    Ok(())
}

#[tokio::test]
async fn test_meta_extension_is_reserved() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;
    assert!(backend
        .write_secret("app/db.meta", one("password", "x"))
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn test_dotted_names_keep_separate_metadata() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;

    backend
        .update_metadata("app/db.v1", one("rotation_period_months", "1"))
        .await?;
    backend
        .update_metadata("app/db.v2", one("rotation_period_months", "12"))
        .await?;

    assert_eq!(
        backend.read_metadata("app/db.v1").await?["rotation_period_months"],
        "1"
    );
    assert_eq!(
        backend.read_metadata("app/db.v2").await?["rotation_period_months"],
        "12"
    );
    Ok(())
}

#[tokio::test]
async fn test_metadata_written_before_0_7_1_is_still_read() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;
    // Old layout: the extension was replaced, so app/db.creds -> app/db.meta.
    std::fs::create_dir_all(temp_dir.path().join("app"))?;
    std::fs::write(
        temp_dir.path().join("app/db.meta"),
        "rotation_enabled:true\n",
    )?;

    assert_eq!(
        backend.read_metadata("app/db.creds").await?["rotation_enabled"],
        "true"
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_secret_files_are_owner_only_and_written_atomically() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;
    let file = temp_dir.path().join("app/db");

    backend
        .write_secret("app/db", one("password", "first"))
        .await?;
    // An existing world-readable file must not stay world-readable.
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
    backend
        .write_secret("app/db", one("password", "second"))
        .await?;

    assert_eq!(
        std::fs::metadata(&file)?.permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        backend.read_secret("app/db").await?.data["password"],
        "second"
    );
    // No temporary files left behind.
    let leftovers: Vec<_> = std::fs::read_dir(temp_dir.path().join("app"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_symlinked_directory_pointing_outside_is_not_followed() -> Result<()> {
    let root = TempDir::new()?;
    let base = root.path().join("secrets");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside)?;
    std::fs::write(outside.join("db"), "password:keep-me\n")?;
    let backend = FileBackend::new(&base)?;
    // secrets/app -> ../outside: every path component is "normal".
    std::os::unix::fs::symlink(&outside, base.join("app"))?;

    assert!(backend.read_secret("app/db").await.is_err());
    assert!(backend
        .write_secret("app/db", one("password", "x"))
        .await
        .is_err());
    assert!(backend
        .write_secret("app/new", one("password", "x"))
        .await
        .is_err());
    assert!(backend
        .update_metadata("app/db", one("k", "v"))
        .await
        .is_err());

    assert_eq!(
        std::fs::read_to_string(outside.join("db"))?,
        "password:keep-me\n"
    );
    assert!(!outside.join("new").exists());
    assert!(!outside.join("db.meta").exists());
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_symlinked_file_pointing_outside_is_not_read_or_written_through() -> Result<()> {
    let root = TempDir::new()?;
    let base = root.path().join("secrets");
    let outside = root.path().join("victim");
    std::fs::write(&outside, "password:keep-me\n")?;
    let backend = FileBackend::new(&base)?;
    std::os::unix::fs::symlink(&outside, base.join("db"))?;

    assert!(backend.read_secret("db").await.is_err());
    // A write may replace the link itself, but must never change its target.
    let _ = backend.write_secret("db", one("password", "x")).await;
    assert_eq!(std::fs::read_to_string(&outside)?, "password:keep-me\n");
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_listing_skips_symlinks_that_leave_the_directory() -> Result<()> {
    let root = TempDir::new()?;
    let base = root.path().join("secrets");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside)?;
    std::fs::write(outside.join("stolen"), "password:x\n")?;
    let backend = FileBackend::new(&base)?;
    backend.write_secret("app/db", one("password", "x")).await?;
    std::os::unix::fs::symlink(&outside, base.join("escape"))?;

    let listed = backend.list_secrets("").await?;
    assert_eq!(listed, vec!["app/db".to_string()]);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_symlinks_that_stay_inside_still_work() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = FileBackend::new(temp_dir.path())?;
    backend
        .write_secret("real/db", one("password", "inside"))
        .await?;
    std::os::unix::fs::symlink("real", temp_dir.path().join("alias"))?;

    assert_eq!(
        backend.read_secret("alias/db").await?.data["password"],
        "inside"
    );
    Ok(())
}
