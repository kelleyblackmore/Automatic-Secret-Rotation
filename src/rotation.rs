use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::backends::SecretBackend;
use crate::targets::Target;

const ROTATION_METADATA_KEY: &str = "rotation_enabled";
const LAST_ROTATED_KEY: &str = "last_rotated";
const ROTATION_PERIOD_KEY: &str = "rotation_period_months";

/// Check if a secret needs rotation based on metadata
pub fn needs_rotation(
    metadata: &Option<HashMap<String, String>>,
    default_period_months: u32,
) -> bool {
    let Some(meta) = metadata else {
        return false;
    };

    // Check if rotation is enabled
    if meta.get(ROTATION_METADATA_KEY) != Some(&"true".to_string()) {
        return false;
    }

    // Get last rotation time
    let last_rotated = match meta.get(LAST_ROTATED_KEY) {
        Some(date_str) => match DateTime::parse_from_rfc3339(date_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => {
                warn!("Failed to parse last_rotated date: {}", date_str);
                return true; // Rotate if we can't parse the date
            }
        },
        None => {
            // No rotation date set, should rotate
            return true;
        }
    };

    // Get rotation period (use custom or default)
    let period_months = meta
        .get(ROTATION_PERIOD_KEY)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default_period_months as i64);

    // Calculate if rotation is due
    let rotation_due = last_rotated + Duration::days(period_months * 30);
    let now = Utc::now();

    now >= rotation_due
}

/// Generate a random secret
pub fn generate_secret(length: usize) -> String {
    const CHARSET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Rotate a secret and update metadata
pub async fn rotate_secret(
    backend: &dyn SecretBackend,
    path: &str,
    secret_length: usize,
) -> Result<String> {
    rotate_secret_with_target(backend, path, secret_length, None, None).await
}

/// Rotate a secret and optionally update target password (database, API, etc.)
pub async fn rotate_secret_with_target(
    backend: &dyn SecretBackend,
    path: &str,
    secret_length: usize,
    target: Option<&dyn Target>,
    target_username: Option<&str>,
) -> Result<String> {
    info!("Rotating secret at {} ({})", path, backend.backend_type());

    // Read current secret
    let current = backend
        .read_secret(path)
        .await
        .context("Failed to read current secret")?;

    // Generate new secret
    let new_secret = generate_secret(secret_length);

    // Update secret data
    let mut new_data = current.data.clone();
    // Determine which key to update - look for common key names
    let key_to_update = new_data
        .keys()
        .find(|k| {
            let lower = k.to_lowercase();
            lower.contains("password")
                || lower.contains("secret")
                || lower.contains("key")
                || lower.contains("token")
        })
        .cloned()
        .unwrap_or_else(|| "secret".to_string());

    new_data.insert(key_to_update.clone(), new_secret.clone());

    // Write updated secret
    backend
        .write_secret(path, new_data)
        .await
        .context("Failed to write rotated secret")?;

    // Update target password if configured
    if let Some(target) = target {
        if let Some(username) = target_username {
            info!(
                "Updating {} password for user: {}",
                target.target_type(),
                username
            );
            target
                .update_password(username, &new_secret)
                .await
                .with_context(|| format!("Failed to update {} password", target.target_type()))?;

            // Optionally verify the new password works
            target
                .verify_connection(username, &new_secret, None)
                .await
                .with_context(|| {
                    format!("Failed to verify new {} password", target.target_type())
                })?;
        }
    }

    // Update metadata with rotation timestamp
    let mut metadata = match backend.read_metadata(path).await {
        Ok(existing) => existing,
        Err(e) => {
            warn!(
                "Failed to read existing metadata for {}: {}. Proceeding with defaults.",
                path, e
            );
            HashMap::new()
        }
    };

    metadata.insert(ROTATION_METADATA_KEY.to_string(), "true".to_string());
    metadata.insert(LAST_ROTATED_KEY.to_string(), Utc::now().to_rfc3339());

    backend
        .update_metadata(path, metadata)
        .await
        .context("Failed to update metadata")?;

    info!("Successfully rotated secret at {}", path);
    Ok(new_secret)
}

/// Flag a secret for automatic rotation
pub async fn flag_for_rotation(
    backend: &dyn SecretBackend,
    path: &str,
    period_months: u32,
) -> Result<()> {
    info!(
        "Flagging secret at {} ({}) for rotation every {} months",
        path,
        backend.backend_type(),
        period_months
    );

    let mut metadata = HashMap::new();
    metadata.insert(ROTATION_METADATA_KEY.to_string(), "true".to_string());
    metadata.insert(LAST_ROTATED_KEY.to_string(), Utc::now().to_rfc3339());
    metadata.insert(ROTATION_PERIOD_KEY.to_string(), period_months.to_string());

    backend
        .update_metadata(path, metadata)
        .await
        .context("Failed to update metadata")?;

    info!("Successfully flagged secret at {} for rotation", path);
    Ok(())
}

/// Scan for secrets that need rotation
pub async fn scan_for_rotation(
    backend: &dyn SecretBackend,
    path: &str,
    default_period: u32,
) -> Result<Vec<String>> {
    info!(
        "Scanning for secrets needing rotation in {} ({})",
        if path.is_empty() { "/" } else { path },
        backend.backend_type()
    );

    let secrets = backend
        .list_secrets(path)
        .await
        .context("Failed to list secrets")?;

    let mut needs_rotation_list = Vec::new();

    for secret in secrets {
        let secret_path = if path.is_empty() {
            secret.clone()
        } else {
            format!("{}/{}", path, secret)
        };

        match backend.read_metadata(&secret_path).await {
            Ok(metadata) => {
                if needs_rotation(&Some(metadata), default_period) {
                    needs_rotation_list.push(secret_path);
                }
            }
            Err(e) => {
                warn!("Failed to read metadata for {}: {}", secret_path, e);
            }
        }
    }

    Ok(needs_rotation_list)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
