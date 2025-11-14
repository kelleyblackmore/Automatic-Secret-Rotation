use anyhow::{Context, Result};
use std::sync::Arc;
use tokio_postgres::{Client, NoTls};
use tracing::{debug, info};

use crate::config::PostgresTargetConfig;
use crate::targets::target::Target;

/// PostgreSQL database target for password updates
pub struct PostgresTarget {
    config: Arc<PostgresTargetConfig>,
    admin_client: Client,
}

impl PostgresTarget {
    /// Create a new PostgresTarget with admin credentials
    pub async fn new(config: &PostgresTargetConfig, admin_password: &str) -> Result<Self> {
        info!(
            "Connecting to PostgreSQL at {}:{}",
            config.host, config.port
        );

        let connection_string = Self::build_connection_string(
            &config.host,
            config.port,
            &config.username,
            admin_password,
            &config.database,
            &config.ssl_mode,
        );

        let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
            .await
            .context("Failed to connect to PostgreSQL")?;

        // Spawn connection handler
        let _connection_handle = tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("PostgreSQL connection error: {}", e);
            }
        });

        // Test the connection
        client
            .query_one("SELECT version()", &[])
            .await
            .context("Failed to verify PostgreSQL connection")?;

        info!("Successfully connected to PostgreSQL");

        Ok(Self {
            config: Arc::new(config.clone()),
            admin_client: client,
        })
    }

    /// Build PostgreSQL connection string
    fn build_connection_string(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        database: &str,
        ssl_mode: &str,
    ) -> String {
        format!(
            "host={} port={} user={} password={} dbname={} sslmode={}",
            host, port, username, password, database, ssl_mode
        )
    }

    /// Quote PostgreSQL identifier to prevent SQL injection
    fn quote_identifier(identifier: &str) -> String {
        // PostgreSQL identifiers are case-insensitive unless quoted
        // We'll quote them to be safe and preserve case
        format!("\"{}\"", identifier.replace("\"", "\"\""))
    }
}

#[async_trait::async_trait]
impl Target for PostgresTarget {
    async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        info!("Updating password for PostgreSQL user: {}", username);

        // Escape single quotes in password
        let escaped_password = new_password.replace("'", "''");

        // Use ALTER USER to change password
        let query = format!(
            "ALTER USER {} WITH PASSWORD '{}'",
            Self::quote_identifier(username),
            escaped_password
        );

        debug!("Executing: ALTER USER {} WITH PASSWORD '***'", username);

        self.admin_client
            .execute(&query, &[])
            .await
            .context("Failed to update PostgreSQL password")?;

        info!("Successfully updated password for user: {}", username);
        Ok(())
    }

    async fn verify_connection(
        &self,
        username: &str,
        password: &str,
        database: Option<&str>,
    ) -> Result<()> {
        info!("Verifying connection for user: {}", username);

        let db_name = database.unwrap_or(&self.config.database);
        let connection_string = Self::build_connection_string(
            &self.config.host,
            self.config.port,
            username,
            password,
            db_name,
            &self.config.ssl_mode,
        );

        // Try to connect with new credentials
        let (test_client, test_connection) = tokio_postgres::connect(&connection_string, NoTls)
            .await
            .context("Failed to verify new password - connection failed")?;

        // Test with a simple query
        test_client
            .query_one("SELECT 1", &[])
            .await
            .context("Failed to verify new password - query failed")?;

        // Close the test connection by dropping it
        drop(test_connection);

        info!("Successfully verified new password for user: {}", username);
        Ok(())
    }

    fn target_type(&self) -> &'static str {
        "postgres"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_identifier() {
        assert_eq!(
            PostgresTarget::quote_identifier("test_user"),
            "\"test_user\""
        );
        assert_eq!(
            PostgresTarget::quote_identifier("user\"name"),
            "\"user\"\"name\""
        );
    }

    #[test]
    fn test_build_connection_string() {
        let conn_str = PostgresTarget::build_connection_string(
            "localhost",
            5432,
            "postgres",
            "password",
            "postgres",
            "prefer",
        );
        assert!(conn_str.contains("host=localhost"));
        assert!(conn_str.contains("port=5432"));
        assert!(conn_str.contains("user=postgres"));
        assert!(conn_str.contains("password=password"));
        assert!(conn_str.contains("dbname=postgres"));
        assert!(conn_str.contains("sslmode=prefer"));
    }
}
