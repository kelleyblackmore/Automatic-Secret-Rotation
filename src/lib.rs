//! Automatic Secret Rotation Library
//!
//! A library for automatic secret rotation with support for multiple backends.

pub mod backends;
pub mod config;
pub mod env_updater;
pub mod rotation;
pub mod targets;

pub use backends::Backend;
pub use config::Config;
pub use rotation::{flag_for_rotation, generate_secret, rotate_secret, scan_for_rotation};

