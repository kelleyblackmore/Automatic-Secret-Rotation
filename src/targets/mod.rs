//! Password update target implementations
//!
//! This module provides abstractions and implementations for different password update targets.
//! Targets are systems where passwords need to be updated when secrets are rotated:
//! - **postgres**: PostgreSQL database (always compiled)
//! - **api**: REST API endpoint (always compiled)
//! - **mysql**: MySQL / MariaDB database (requires `--features mysql`)
//! - **gitlab**: GitLab CI/CD variable (requires `--features gitlab`)
//! - **github**: GitHub Actions secret or variable (requires `--features github`)

mod api;
mod github;
mod gitlab;
mod mysql;
mod postgres;
mod target;

pub use api::ApiTarget;
pub use postgres::PostgresTarget;
pub use target::Target;

#[cfg(feature = "mysql")]
pub use mysql::MysqlTarget;

#[cfg(feature = "gitlab")]
pub use gitlab::GitLabTarget;

#[cfg(feature = "github")]
pub use github::GitHubTarget;

/// Target type enumeration for type-safe target selection by library consumers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TargetType {
    Postgres,
    Api,
    Mysql,
    GitLab,
    GitHub,
}

impl std::str::FromStr for TargetType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgres" | "postgresql" => Ok(TargetType::Postgres),
            "api" => Ok(TargetType::Api),
            "mysql" | "mariadb" => Ok(TargetType::Mysql),
            "gitlab" => Ok(TargetType::GitLab),
            "github" => Ok(TargetType::GitHub),
            _ => Err(format!(
                "Unknown target type: {}. Supported: postgres, api, mysql, gitlab, github",
                s
            )),
        }
    }
}

/// Type alias for target trait object
pub type TargetInstance = Box<dyn Target>;
