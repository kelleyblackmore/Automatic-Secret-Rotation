use anyhow::Result;
use chrono::{Duration, Utc};
use secret_rotator::backends::{FileBackend, SecretBackend};
use secret_rotator::rotation::{flag_for_rotation, rotate_secret, scan_for_rotation};
use secret_rotator::rotation::{rotate_secret_with_target, rotate_secret_with_targets};
use secret_rotator::targets::Target;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Mock target
// ---------------------------------------------------------------------------

struct MockTarget {
    update_calls: Arc<Mutex<Vec<(String, String)>>>,
    verify_calls: Arc<Mutex<Vec<(String, String)>>>,
    fail_on_update: bool,
    needs_username: bool,
}

impl MockTarget {
    fn new() -> Self {
        Self {
            update_calls: Arc::new(Mutex::new(Vec::new())),
            verify_calls: Arc::new(Mutex::new(Vec::new())),
            fail_on_update: false,
            needs_username: false,
        }
    }

    fn update_call_count(&self) -> usize {
        self.update_calls.lock().unwrap().len()
    }

    fn last_updated_password(&self) -> Option<String> {
        self.update_calls
            .lock()
            .unwrap()
            .last()
            .map(|(_, p)| p.clone())
    }
}

#[async_trait::async_trait]
impl Target for MockTarget {
    async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        if self.fail_on_update {
            anyhow::bail!("mock target update failure");
        }
        self.update_calls
            .lock()
            .unwrap()
            .push((username.to_string(), new_password.to_string()));
        Ok(())
    }

    async fn verify_connection(
        &self,
        username: &str,
        password: &str,
        _extra: Option<&str>,
    ) -> Result<()> {
        self.verify_calls
            .lock()
            .unwrap()
            .push((username.to_string(), password.to_string()));
        Ok(())
    }

    fn target_type(&self) -> &'static str {
        "mock"
    }

    fn requires_username(&self) -> bool {
        self.needs_username
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn write_secret(backend: &FileBackend, path: &str, key: &str, value: &str) {
    let mut data = HashMap::new();
    data.insert(key.to_string(), value.to_string());
    backend.write_secret(path, data).await.unwrap();
}

async fn flag_secret_as_overdue(backend: &FileBackend, path: &str) {
    let mut meta = HashMap::new();
    meta.insert("rotation_enabled".to_string(), "true".to_string());
    let old = (Utc::now() - Duration::days(200)).to_rfc3339();
    meta.insert("last_rotated".to_string(), old);
    backend.update_metadata(path, meta).await.unwrap();
}

async fn flag_secret_as_fresh(backend: &FileBackend, path: &str) {
    let mut meta = HashMap::new();
    meta.insert("rotation_enabled".to_string(), "true".to_string());
    meta.insert("last_rotated".to_string(), Utc::now().to_rfc3339());
    backend.update_metadata(path, meta).await.unwrap();
}

// ---------------------------------------------------------------------------
// rotate_secret
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rotate_secret_returns_secret_of_correct_length() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "old").await;

    let new = rotate_secret(&backend, "app/db", 24).await?;
    assert_eq!(new.len(), 24);
    Ok(())
}

#[tokio::test]
async fn test_rotate_secret_stores_new_value_in_backend() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "old").await;

    let new = rotate_secret(&backend, "app/db", 32).await?;

    let stored = backend.read_secret("app/db").await?;
    assert_eq!(stored.data.get("password"), Some(&new));
    assert_ne!(new, "old");
    Ok(())
}

#[tokio::test]
async fn test_rotate_secret_each_call_generates_different_value() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "original").await;

    let first = rotate_secret(&backend, "app/db", 32).await?;
    let second = rotate_secret(&backend, "app/db", 32).await?;
    assert_ne!(
        first, second,
        "consecutive rotations should produce distinct secrets"
    );
    Ok(())
}

#[tokio::test]
async fn test_rotate_secret_updates_rotation_metadata() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/token", "token", "old").await;

    rotate_secret(&backend, "app/token", 32).await?;

    let meta = backend.read_metadata("app/token").await?;
    assert_eq!(
        meta.get("rotation_enabled"),
        Some(&"true".to_string()),
        "rotation_enabled should be set to true"
    );
    assert!(
        meta.contains_key("last_rotated"),
        "last_rotated timestamp should be written"
    );
    Ok(())
}

#[tokio::test]
async fn test_rotate_secret_picks_up_existing_key_name() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;

    let mut data = HashMap::new();
    data.insert("api_key".to_string(), "oldapikey".to_string());
    backend.write_secret("app/apikey", data).await?;

    rotate_secret(&backend, "app/apikey", 32).await?;

    let stored = backend.read_secret("app/apikey").await?;
    assert!(
        stored.data.contains_key("api_key"),
        "key name 'api_key' should be preserved"
    );
    assert_ne!(stored.data["api_key"], "oldapikey");
    Ok(())
}

// ---------------------------------------------------------------------------
// rotate_secret_with_target
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rotate_with_target_calls_update_and_verify() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "old").await;

    let target = MockTarget::new();

    let new =
        rotate_secret_with_target(&backend, "app/db", 32, Some(&target), Some("admin")).await?;

    assert_eq!(target.update_call_count(), 1);
    assert_eq!(target.last_updated_password(), Some(new.clone()));

    let verify_calls = target.verify_calls.lock().unwrap();
    assert_eq!(verify_calls.len(), 1);
    assert_eq!(verify_calls[0].0, "admin");
    assert_eq!(verify_calls[0].1, new);
    Ok(())
}

#[tokio::test]
async fn test_rotate_with_no_target_still_rotates_backend() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "old").await;

    let new = rotate_secret_with_target(&backend, "app/db", 32, None, None).await?;
    assert_eq!(new.len(), 32);

    let stored = backend.read_secret("app/db").await?;
    assert_eq!(stored.data.get("password"), Some(&new));
    Ok(())
}

#[tokio::test]
async fn test_rotate_with_target_that_requires_username_fails_when_none_given() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "old").await;

    let mut target = MockTarget::new();
    target.needs_username = true;

    let result = rotate_secret_with_target(&backend, "app/db", 32, Some(&target), None).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.to_lowercase().contains("username"),
        "error should mention username: {msg}"
    );
    Ok(())
}

#[tokio::test]
async fn test_rotate_with_target_username_not_required_works_without_one() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/var", "secret", "old").await;

    let target = MockTarget::new();

    let new = rotate_secret_with_target(&backend, "app/var", 32, Some(&target), None).await?;

    let calls = target.update_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0, "",
        "username should be empty string for username-free targets"
    );
    assert_eq!(calls[0].1, new);
    Ok(())
}

// ---------------------------------------------------------------------------
// rotate_secret_with_targets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rotate_with_multiple_targets_all_receive_same_secret() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "old").await;

    let t1 = MockTarget::new();
    let t2 = MockTarget::new();
    let calls1 = Arc::clone(&t1.update_calls);
    let calls2 = Arc::clone(&t2.update_calls);

    let targets: Vec<Box<dyn Target>> = vec![Box::new(t1), Box::new(t2)];

    let new = rotate_secret_with_targets(&backend, "app/db", 32, &targets, None).await?;

    let c1 = calls1.lock().unwrap();
    let c2 = calls2.lock().unwrap();

    assert_eq!(c1.len(), 1, "first target should be called once");
    assert_eq!(c2.len(), 1, "second target should be called once");
    assert_eq!(c1[0].1, new, "first target should get the new secret");
    assert_eq!(
        c2[0].1, new,
        "second target should receive the same new secret"
    );
    Ok(())
}

#[tokio::test]
async fn test_rotate_with_single_target_in_slice() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/db", "password", "old").await;

    let target = MockTarget::new();
    let calls = Arc::clone(&target.update_calls);
    let targets: Vec<Box<dyn Target>> = vec![Box::new(target)];

    rotate_secret_with_targets(&backend, "app/db", 32, &targets, None).await?;

    assert_eq!(calls.lock().unwrap().len(), 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// flag_for_rotation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_flag_for_rotation_sets_enabled_and_period() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/cred", "secret", "val").await;

    flag_for_rotation(&backend, "app/cred", 3).await?;

    let meta = backend.read_metadata("app/cred").await?;
    assert_eq!(meta.get("rotation_enabled"), Some(&"true".to_string()));
    assert_eq!(meta.get("rotation_period_months"), Some(&"3".to_string()));
    assert!(meta.contains_key("last_rotated"));
    Ok(())
}

#[tokio::test]
async fn test_flag_for_rotation_different_periods() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;
    write_secret(&backend, "app/cred", "secret", "val").await;

    for period in [1u32, 6, 12, 24] {
        flag_for_rotation(&backend, "app/cred", period).await?;
        let meta = backend.read_metadata("app/cred").await?;
        assert_eq!(
            meta.get("rotation_period_months"),
            Some(&period.to_string()),
            "period {period} should be stored"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// scan_for_rotation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_scan_finds_overdue_flagged_secret() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;

    write_secret(&backend, "app/overdue", "password", "pass").await;
    flag_secret_as_overdue(&backend, "app/overdue").await;

    let due = scan_for_rotation(&backend, "", 6).await?;

    assert_eq!(due.len(), 1);
    assert!(
        due[0].contains("overdue"),
        "overdue secret should be in results: {due:?}"
    );
    Ok(())
}

#[tokio::test]
async fn test_scan_does_not_return_fresh_secret() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;

    write_secret(&backend, "app/fresh", "password", "pass").await;
    flag_secret_as_fresh(&backend, "app/fresh").await;

    let due = scan_for_rotation(&backend, "", 6).await?;
    assert!(due.is_empty(), "fresh secret should not be in results");
    Ok(())
}

#[tokio::test]
async fn test_scan_skips_unflagged_secrets() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;

    write_secret(&backend, "app/unflagged", "password", "pass").await;

    let due = scan_for_rotation(&backend, "", 6).await?;
    assert!(
        due.is_empty(),
        "secret with no rotation metadata should not appear"
    );
    Ok(())
}

#[tokio::test]
async fn test_scan_returns_only_overdue_from_mixed_list() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;

    write_secret(&backend, "app/overdue", "password", "p1").await;
    flag_secret_as_overdue(&backend, "app/overdue").await;

    write_secret(&backend, "app/fresh", "password", "p2").await;
    flag_secret_as_fresh(&backend, "app/fresh").await;

    write_secret(&backend, "app/unflagged", "password", "p3").await;

    let due = scan_for_rotation(&backend, "", 6).await?;

    assert_eq!(due.len(), 1);
    assert!(
        due[0].contains("overdue"),
        "only overdue secret should appear: {due:?}"
    );
    Ok(())
}

#[tokio::test]
async fn test_scan_empty_path_returns_empty_for_empty_backend() -> Result<()> {
    let temp = TempDir::new()?;
    let backend = FileBackend::new(temp.path())?;

    let due = scan_for_rotation(&backend, "", 6).await?;
    assert!(due.is_empty());
    Ok(())
}
