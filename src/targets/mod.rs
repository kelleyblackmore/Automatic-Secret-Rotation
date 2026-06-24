//! Password update target implementations
//!
//! This module provides abstractions and implementations for different password update targets.
//! Targets are systems where passwords need to be updated when secrets are rotated:
//! - **postgres**: PostgreSQL database (always compiled)
//! - **api**: REST API endpoint (always compiled)
//! - **mysql**: MySQL / MariaDB database (requires `--features mysql`)

mod api;
mod mysql;
mod postgres;
mod target;

pub use api::ApiTarget;
pub use postgres::PostgresTarget;
pub use target::Target;

#[cfg(feature = "mysql")]
pub use mysql::MysqlTarget;

/// Target type enumeration for type-safe target selection by library consumers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TargetType {
    Postgres,
    Api,
    Mysql,
}

impl std::str::FromStr for TargetType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgres" | "postgresql" => Ok(TargetType::Postgres),
            "api" => Ok(TargetType::Api),
            "mysql" | "mariadb" => Ok(TargetType::Mysql),
            _ => Err(format!(
                "Unknown target type: {}. Supported: postgres, api, mysql",
                s
            )),
        }
    }
}

/// Type alias for target trait object
pub type TargetInstance = Box<dyn Target>;
