use chrono::{Duration, Utc};
use secret_rotator::rotation::{generate_secret, needs_rotation};
use std::collections::HashMap;

#[test]
fn test_generate_secret() {
    let secret = generate_secret(32);
    assert_eq!(secret.len(), 32);

    let secret2 = generate_secret(32);
    assert_ne!(secret, secret2); // Should be different each time
}

#[test]
fn test_needs_rotation_no_metadata() {
    assert!(!needs_rotation(&None, 6));
}

#[test]
fn test_needs_rotation_not_enabled() {
    let mut meta = HashMap::new();
    meta.insert("rotation_enabled".to_string(), "false".to_string());
    assert!(!needs_rotation(&Some(meta), 6));
}

#[test]
fn test_needs_rotation_no_date() {
    let mut meta = HashMap::new();
    meta.insert("rotation_enabled".to_string(), "true".to_string());
    assert!(needs_rotation(&Some(meta), 6));
}

#[test]
fn test_needs_rotation_recent() {
    let mut meta = HashMap::new();
    meta.insert("rotation_enabled".to_string(), "true".to_string());
    meta.insert("last_rotated".to_string(), Utc::now().to_rfc3339());
    assert!(!needs_rotation(&Some(meta), 6));
}

#[test]
fn test_needs_rotation_old() {
    let mut meta = HashMap::new();
    meta.insert("rotation_enabled".to_string(), "true".to_string());
    let old_date = Utc::now() - Duration::days(200);
    meta.insert("last_rotated".to_string(), old_date.to_rfc3339());
    assert!(needs_rotation(&Some(meta), 6));
}
