use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
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

/// Shortest secret `asr` will generate. A misconfigured length (0 sets an
/// empty password) must fail instead of weakening the credential.
pub const MIN_SECRET_LENGTH: usize = 16;

/// Reject secret lengths below [`MIN_SECRET_LENGTH`].
pub fn check_secret_length(length: usize) -> Result<()> {
    if length < MIN_SECRET_LENGTH {
        anyhow::bail!(
            "secret_length is {}; it must be at least {}",
            length,
            MIN_SECRET_LENGTH
        );
    }
    Ok(())
}

/// Pick the field of a multi-field secret that holds the credential.
///
/// Secrets are `HashMap`s, whose iteration order changes between runs, so
/// "first match" overwrote a different field each time (e.g. `api_key`
/// instead of `password`). The choice is now fixed: the first of these name
/// fragments to match wins, ties go to the alphabetically first key, and a
/// single-field secret uses its only field.
pub fn select_secret_key(data: &HashMap<String, String>) -> Option<String> {
    const PREFERENCE: [&str; 4] = ["password", "secret", "token", "key"];
    let mut keys: Vec<&String> = data.keys().collect();
    keys.sort();
    PREFERENCE
        .iter()
        .find_map(|fragment| {
            keys.iter()
                .find(|k| k.to_lowercase().contains(fragment))
                .map(|k| (*k).clone())
        })
        .or_else(|| (keys.len() == 1).then(|| keys[0].clone()))
}

/// Generate a random secret
pub fn generate_secret(length: usize) -> String {
    // rand 0.10 moved the range-sampling helpers off `Rng` (now a re-export of
    // `rand_core::Rng`) and onto the `RngExt` extension trait.
    use rand::RngExt;
    const CHARSET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
    let mut rng = rand::rng();
    (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
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

/// Rotate a secret and update password on all provided targets (multi-target support).
pub async fn rotate_secret_with_targets(
    backend: &dyn SecretBackend,
    path: &str,
    secret_length: usize,
    targets: &[Box<dyn Target>],
    target_username: Option<&str>,
) -> Result<String> {
    let target_refs: Vec<&dyn Target> = targets.iter().map(|t| t.as_ref()).collect();
    rotate(backend, path, secret_length, &target_refs, target_username).await
}

/// Rotate a secret and optionally update target password (database, API, etc.)
pub async fn rotate_secret_with_target(
    backend: &dyn SecretBackend,
    path: &str,
    secret_length: usize,
    target: Option<&dyn Target>,
    target_username: Option<&str>,
) -> Result<String> {
    let targets: Vec<&dyn Target> = target.into_iter().collect();
    rotate(backend, path, secret_length, &targets, target_username).await
}

/// Rotate the secret at `path` and push the new value to every target.
///
/// Order matters, because apps read the password from the backend:
///
/// 1. Check every precondition before anything changes.
/// 2. Update and verify each target in turn.
/// 3. Only then write the backend.
///
/// If a target or the backend write fails, every target that may already
/// hold the new value is set back to the old one, so the backend and the
/// targets keep agreeing. Errors never include either secret value: they are
/// recorded in the audit log and sent to webhooks.
async fn rotate(
    backend: &dyn SecretBackend,
    path: &str,
    secret_length: usize,
    targets: &[&dyn Target],
    target_username: Option<&str>,
) -> Result<String> {
    info!("Rotating secret at {} ({})", path, backend.backend_type());
    check_secret_length(secret_length)?;

    if target_username.is_none() {
        if let Some(target) = targets.iter().find(|t| t.requires_username()) {
            anyhow::bail!(
                "{} target requires a username — pass --target-username or set \
                 target_username in the secret metadata",
                target.target_type()
            );
        }
    }
    let username = target_username.unwrap_or("");

    // Read current secret
    let current = backend
        .read_secret(path)
        .await
        .context("Failed to read current secret")?;

    let key_to_update = select_secret_key(&current.data).unwrap_or_else(|| "secret".to_string());
    let old_secret = current.data.get(&key_to_update).cloned();

    let new_secret = generate_secret(secret_length);

    for (i, target) in targets.iter().enumerate() {
        info!(
            "Updating {} target{}",
            target.target_type(),
            if username.is_empty() {
                String::new()
            } else {
                format!(" for user: {}", username)
            }
        );
        let failure = match target
            .update_password(username, &new_secret)
            .await
            .with_context(|| format!("Failed to update {} target", target.target_type()))
        {
            // The update never applied, so this target still has the old value.
            Err(e) => Some((e, i)),
            Ok(()) => target
                .verify_connection(username, &new_secret, None)
                .await
                .with_context(|| format!("Failed to verify new {} value", target.target_type()))
                .err()
                // The update applied; only verification failed.
                .map(|e| (e, i + 1)),
        };

        if let Some((e, changed)) = failure {
            let rollback = roll_back(&targets[..changed], username, old_secret.as_deref()).await;
            return Err(e.context(format!(
                "Rotation of {} aborted; the backend was not changed. {}",
                path, rollback
            )));
        }
    }

    let mut new_data = current.data.clone();
    new_data.insert(key_to_update, new_secret.clone());

    if let Err(e) = backend.write_secret(path, new_data).await {
        let rollback = roll_back(targets, username, old_secret.as_deref()).await;
        return Err(e.context(format!(
            "Failed to write rotated secret to {}. {}",
            path, rollback
        )));
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
        .context("Secret rotated, but failed to update rotation metadata")?;

    info!("Successfully rotated secret at {}", path);
    Ok(new_secret)
}

/// Set `targets` back to `old_secret` after a failed rotation. Returns a
/// sentence describing the outcome for the caller's error message.
async fn roll_back(targets: &[&dyn Target], username: &str, old_secret: Option<&str>) -> String {
    if targets.is_empty() {
        return "No targets were changed.".to_string();
    }
    let names: Vec<&str> = targets.iter().map(|t| t.target_type()).collect();
    let Some(old_secret) = old_secret else {
        return format!(
            "The secret had no previous value, so these targets could not be rolled back \
             and may hold an unrecorded value: {}.",
            names.join(", ")
        );
    };

    let mut failed = Vec::new();
    for target in targets {
        if let Err(e) = target.update_password(username, old_secret).await {
            warn!(
                "Rollback of {} target failed: {:#}",
                target.target_type(),
                e
            );
            failed.push(format!("{} ({:#})", target.target_type(), e));
        }
    }
    if failed.is_empty() {
        format!("Rolled back to the previous value: {}.", names.join(", "))
    } else {
        format!(
            "ROLLBACK FAILED for: {}. Those targets may hold a password that is not in \
             the backend; reset them manually.",
            failed.join("; ")
        )
    }
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
    use super::select_secret_key;
    use std::collections::HashMap;

    fn data(keys: &[&str]) -> HashMap<String, String> {
        keys.iter()
            .map(|k| (k.to_string(), "v".to_string()))
            .collect()
    }

    #[test]
    fn select_secret_key_prefers_password_over_other_matches() {
        // Run many times: HashMap order varies, the choice must not.
        for _ in 0..50 {
            let d = data(&["username", "api_key", "password", "token"]);
            assert_eq!(select_secret_key(&d).as_deref(), Some("password"));
        }
        assert_eq!(
            select_secret_key(&data(&["api_key", "auth_token"])).as_deref(),
            Some("auth_token")
        );
        assert_eq!(
            select_secret_key(&data(&["DB_PASSWORD", "admin_password"])).as_deref(),
            Some("DB_PASSWORD")
        );
    }

    #[test]
    fn check_secret_length_rejects_short_secrets() {
        assert!(super::check_secret_length(0).is_err());
        assert!(super::check_secret_length(15).is_err());
        assert!(super::check_secret_length(16).is_ok());
    }

    #[test]
    fn select_secret_key_uses_the_only_field_or_gives_up() {
        assert_eq!(
            select_secret_key(&data(&["value"])).as_deref(),
            Some("value")
        );
        assert_eq!(select_secret_key(&data(&["host", "user"])), None);
        assert_eq!(select_secret_key(&data(&[])), None);
    }
}
