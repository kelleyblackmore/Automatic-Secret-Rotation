//! Password update target implementations
//!
//! This module provides abstractions and implementations for different password update targets.
//! Targets are systems where passwords need to be updated when secrets are rotated:
//! - Databases (PostgreSQL, MySQL, etc.)
//! - APIs (REST APIs that manage user passwords)
//! - Applications (LDAP, Active Directory, etc.)

mod api;
mod postgres;
mod target;

pub use api::ApiTarget;
pub use postgres::PostgresTarget;
pub use target::Target;

/// Target type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Reserved for future use (e.g., type-safe target selection)
pub enum TargetType {
    Postgres,
    Api,
}

impl std::str::FromStr for TargetType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgres" | "postgresql" => Ok(TargetType::Postgres),
            "api" => Ok(TargetType::Api),
            _ => Err(format!(
                "Unknown target type: {}. Supported: postgres, api",
                s
            )),
        }
    }
}

/// Type alias for target trait object
pub type TargetInstance = Box<dyn Target>;
