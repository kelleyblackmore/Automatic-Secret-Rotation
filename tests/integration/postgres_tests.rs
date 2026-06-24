//! Integration tests against a live PostgreSQL instance.
//!
//! Requires the Docker Compose stack to be running:
//!   docker compose -f tests/integration/docker-compose.yml up -d
//!
//! Run with:
//!   cargo test --features integration -- --ignored postgres

#![cfg(feature = "integration")]

use anyhow::Result;
use secret_rotator::config::PostgresTargetConfig;
use secret_rotator::targets::{PostgresTarget, Target};

fn test_pg_config(username: &str, password: &str) -> PostgresTargetConfig {
    PostgresTargetConfig {
        host: "127.0.0.1".to_string(),
        port: 5432,
        database: "testdb".to_string(),
        username: username.to_string(),
        password_path: None,
        password: Some(password.to_string()),
        ssl_mode: "disable".to_string(),
    }
}

async fn admin_target() -> Result<PostgresTarget> {
    let cfg = test_pg_config("admin", "admin_password");
    PostgresTarget::new(&cfg, "admin_password").await
}

#[tokio::test]
#[ignore = "requires PostgreSQL docker-compose stack"]
async fn test_postgres_update_and_verify_password() -> Result<()> {
    let target = admin_target().await?;

    // Create a test user
    let test_user = "asr_test_user";
    let initial_password = "initial_password_123";
    let new_password = "rotated_password_456!@#";

    // Create user (ignore error if already exists)
    let _ = target
        .update_password(test_user, initial_password)
        .await;

    // Rotate password
    target.update_password(test_user, new_password).await?;

    // Verify new password works
    target
        .verify_connection(test_user, new_password, Some("testdb"))
        .await?;

    Ok(())
}

#[tokio::test]
#[ignore = "requires PostgreSQL docker-compose stack"]
async fn test_postgres_password_with_special_chars() -> Result<()> {
    let target = admin_target().await?;
    let test_user = "asr_special_user";

    // Passwords with characters that could break connection strings
    let special_passwords = [
        "pass'word",
        r"pass\word",
        "pass word",
        "pass=word",
        r"pass'\word with spaces",
    ];

    for password in &special_passwords {
        target
            .update_password(test_user, password)
            .await
            .with_context(|| format!("Failed to set password: {:?}", password))?;

        target
            .verify_connection(test_user, password, Some("testdb"))
            .await
            .with_context(|| {
                format!(
                    "Failed to verify connection with password: {:?}",
                    password
                )
            })?;
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires PostgreSQL docker-compose stack"]
async fn test_postgres_connection_roundtrip() -> Result<()> {
    let target = admin_target().await?;

    // Verify admin can connect (using own credentials)
    target
        .verify_connection("admin", "admin_password", Some("testdb"))
        .await?;

    Ok(())
}
