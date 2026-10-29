#[cfg(feature = "mysql")]
use crate::util::tls::{parse_ssl_mode, TlsMode};
#[cfg(feature = "mysql")]
use anyhow::{Context, Result};
#[cfg(feature = "mysql")]
use async_trait::async_trait;
#[cfg(feature = "mysql")]
use mysql_async::{Opts, OptsBuilder, Pool, SslOpts};

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
    ssl_mode: Option<String>,
}

#[cfg(feature = "mysql")]
impl MysqlTarget {
    pub async fn new(config: &MysqlTargetConfig, admin_password: &str) -> Result<Self> {
        let opts = build_opts(
            &config.host,
            config.port,
            &config.database,
            &config.username,
            admin_password,
            config.ssl_mode.as_deref(),
        )?;

        let pool = Pool::new(opts);

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
            ssl_mode: config.ssl_mode.clone(),
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

        let escaped_user = quote_string_literal(username).context("Invalid MySQL username")?;
        let escaped_pass = quote_string_literal(new_password).context("Invalid MySQL password")?;

        // ALTER USER is supported in MySQL 5.7.6+/MariaDB 10.2+
        let query = format!(
            "ALTER USER {}@'%' IDENTIFIED BY {}",
            escaped_user, escaped_pass
        );
        conn.query_drop(&query)
            .await
            .with_context(|| format!("Failed to update MySQL password for user '{}'", username))?;

        // Also attempt localhost variant (ignore error if user doesn't exist for localhost)
        let query_local = format!(
            "ALTER USER {}@'localhost' IDENTIFIED BY {}",
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
        let opts = build_opts(
            &self.host,
            self.port,
            &self.database,
            username,
            new_password,
            self.ssl_mode.as_deref(),
        )?;

        let test_pool = Pool::new(opts);
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

/// Quote `value` as a MySQL string literal.
///
/// The username can come from backend metadata, which may be writable by
/// people who are not MySQL admins, and the statement runs as the admin with
/// multi-statements enabled, so this must not be escapable. Doubling `'` is
/// correct whether or not `NO_BACKSLASH_ESCAPES` is set; backslashes and NUL
/// mean different things in the two modes, so they are rejected rather than
/// escaped. Generated passwords never contain either.
#[cfg(feature = "mysql")]
fn quote_string_literal(value: &str) -> Result<String> {
    if value.contains(['\\', '\0']) {
        anyhow::bail!("value contains a backslash or NUL character");
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

/// Build mysql_async Opts with TLS configured from ssl_mode.
///
/// ssl_mode mapping:
///   None / "disable"                         → no TLS
///   "require"                                → TLS, skip cert verification
///   "verify-ca" / "verify-full" / "prefer"  → TLS with full cert verification
#[cfg(feature = "mysql")]
fn build_opts(
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &str,
    ssl_mode: Option<&str>,
) -> Result<Opts> {
    let ssl_opts: Option<SslOpts> = match ssl_mode.map(parse_ssl_mode) {
        None | Some(TlsMode::Disabled) => None,
        Some(TlsMode::RequireNoVerify) => Some(
            SslOpts::default()
                .with_danger_accept_invalid_certs(true)
                .with_danger_skip_domain_validation(true),
        ),
        Some(TlsMode::VerifyFull) => Some(SslOpts::default()),
    };

    let opts = OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(port)
        .db_name(Some(database))
        .user(Some(username))
        .pass(Some(password))
        .ssl_opts(ssl_opts);

    Ok(Opts::from(opts))
}

#[cfg(all(test, feature = "mysql"))]
mod tests {
    use super::quote_string_literal;

    #[test]
    fn quote_string_literal_doubles_single_quotes() {
        assert_eq!(quote_string_literal("app").unwrap(), "'app'");
        assert_eq!(quote_string_literal("o'neil").unwrap(), "'o''neil'");
        assert_eq!(
            quote_string_literal("a'; DROP USER 'root'@'%'; -- ").unwrap(),
            "'a''; DROP USER ''root''@''%''; -- '"
        );
    }

    #[test]
    fn quote_string_literal_rejects_backslash_and_nul() {
        // The old escaping turned `a\'` into `a\\'`, which closes the literal.
        assert!(quote_string_literal("a\\'; DROP USER 'root'@'%'; -- ").is_err());
        assert!(quote_string_literal("a\0b").is_err());
    }

    #[test]
    fn quote_string_literal_accepts_generated_passwords() {
        for _ in 0..200 {
            let pw = crate::rotation::generate_secret(64);
            assert!(quote_string_literal(&pw).is_ok(), "{pw}");
        }
    }
}
