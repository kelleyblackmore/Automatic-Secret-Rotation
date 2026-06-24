#[cfg(feature = "mysql")]
use anyhow::{Context, Result};
#[cfg(feature = "mysql")]
use async_trait::async_trait;
#[cfg(feature = "mysql")]
use mysql_async::{Opts, OptsBuilder, Pool};

#[cfg(feature = "mysql")]
use crate::config::MysqlTargetConfig;
#[cfg(feature = "mysql")]
use crate::targets::target::Target;

#[cfg(feature = "mysql")]
pub struct MysqlTarget {
    pool: Pool,
    host: String,
    port: u16,
    database: String,
}

#[cfg(feature = "mysql")]
impl MysqlTarget {
    pub async fn new(config: &MysqlTargetConfig, admin_password: &str) -> Result<Self> {
        let opts = OptsBuilder::default()
            .ip_or_hostname(config.host.clone())
            .tcp_port(config.port)
            .db_name(Some(config.database.clone()))
            .user(Some(config.username.clone()))
            .pass(Some(admin_password.to_string()));

        let pool = Pool::new(Opts::from(opts));

        // Verify connectivity on creation
        let conn = pool.get_conn().await.with_context(|| {
            format!(
                "Failed to connect to MySQL/MariaDB at {}:{}",
                config.host, config.port
            )
        })?;
        drop(conn);

        Ok(Self {
            pool,
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
        })
    }
}

#[cfg(feature = "mysql")]
#[async_trait]
impl Target for MysqlTarget {
    async fn update_password(&self, username: &str, new_password: &str) -> Result<()> {
        use mysql_async::prelude::Queryable;

        let mut conn = self
            .pool
            .get_conn()
            .await
            .context("Failed to get MySQL connection for password update")?;

        // ALTER USER is supported in MySQL 5.7.6+/MariaDB 10.2+
        let escaped_user = username.replace('\'', "\\'");
        let escaped_pass = new_password.replace('\'', "\\'");

        // Update for any-host variant
        let query = format!(
            "ALTER USER '{}'@'%' IDENTIFIED BY '{}'",
            escaped_user, escaped_pass
        );
        conn.query_drop(&query)
            .await
            .with_context(|| format!("Failed to update MySQL password for user '{}'", username))?;

        // Also attempt localhost variant (ignore error if user doesn't exist for localhost)
        let query_local = format!(
            "ALTER USER '{}'@'localhost' IDENTIFIED BY '{}'",
            escaped_user, escaped_pass
        );
        let _ = conn.query_drop(&query_local).await;

        conn.query_drop("FLUSH PRIVILEGES")
            .await
            .context("Failed to flush MySQL privileges after password update")?;

        Ok(())
    }

    async fn verify_connection(
        &self,
        username: &str,
        new_password: &str,
        _extra: Option<&str>,
    ) -> Result<()> {
        let opts = OptsBuilder::default()
            .ip_or_hostname(self.host.clone())
            .tcp_port(self.port)
            .db_name(Some(self.database.clone()))
            .user(Some(username.to_string()))
            .pass(Some(new_password.to_string()));

        let test_pool = Pool::new(Opts::from(opts));
        let conn = test_pool.get_conn().await.with_context(|| {
            format!(
                "Failed to verify MySQL connection for user '{}' at {}:{}",
                username, self.host, self.port
            )
        })?;
        drop(conn);
        test_pool.disconnect().await.ok();

        Ok(())
    }

    fn target_type(&self) -> &'static str {
        "MySQL/MariaDB"
    }
}
