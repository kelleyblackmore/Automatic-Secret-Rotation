use crate::util::tls::{parse_ssl_mode, TlsMode};
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio_postgres::Client;
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

        let client = pg_connect(&connection_string, &config.ssl_mode).await?;

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

    /// Build PostgreSQL connection string with properly escaped values
    pub fn build_connection_string(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        database: &str,
        ssl_mode: &str,
    ) -> String {
        format!(
            "host={} port={} user={} password={} dbname={} sslmode={}",
            Self::quote_conn_value(host),
            port,
            Self::quote_conn_value(username),
            Self::quote_conn_value(password),
            Self::quote_conn_value(database),
            ssl_mode,
        )
    }

    /// Quote a libpq connection string value, escaping backslashes and single quotes
    fn quote_conn_value(value: &str) -> String {
        if value.contains(['\'', '\\', ' ', '=']) {
            format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
        } else {
            value.to_string()
        }
    }

    /// Quote PostgreSQL identifier to prevent SQL injection
    pub fn quote_identifier(identifier: &str) -> String {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }
}

#[async_trait::async_trait]
impl Target for PostgresTarget {
    async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        info!("Updating password for PostgreSQL user: {}", username);

        let escaped_password = new_password.replace('\'', "''");

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

        let test_client = pg_connect(&connection_string, &self.config.ssl_mode).await?;

        test_client
            .query_one("SELECT 1", &[])
            .await
            .context("Failed to verify new password - query failed")?;

        info!("Successfully verified new password for user: {}", username);
        Ok(())
    }

    fn target_type(&self) -> &'static str {
        "postgres"
    }
}

/// Connect to PostgreSQL using the TLS mode from config.
///
/// ssl_mode mapping:
///   disable                      → NoTls (no encryption)
///   require                      → TLS required, cert/hostname verification skipped
///   prefer / allow / verify-ca / verify-full (default) → TLS with full cert verification
async fn pg_connect(connection_string: &str, ssl_mode: &str) -> Result<Client> {
    if matches!(parse_ssl_mode(ssl_mode), TlsMode::Disabled) {
        let (client, conn) = tokio_postgres::connect(connection_string, tokio_postgres::NoTls)
            .await
            .context("Failed to connect to PostgreSQL (NoTls)")?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("PostgreSQL connection error: {e}");
            }
        });
        return Ok(client);
    }

    let mut builder = native_tls::TlsConnector::builder();
    if matches!(parse_ssl_mode(ssl_mode), TlsMode::RequireNoVerify) {
        builder.danger_accept_invalid_certs(true);
        builder.danger_accept_invalid_hostnames(true);
    }

    let connector = builder
        .build()
        .context("Failed to build TLS connector for PostgreSQL")?;
    let tls = postgres_native_tls::MakeTlsConnector::new(connector);

    let (client, conn) = tokio_postgres::connect(connection_string, tls)
        .await
        .context("Failed to connect to PostgreSQL (TLS)")?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("PostgreSQL connection error: {e}");
        }
    });
    Ok(client)
}
