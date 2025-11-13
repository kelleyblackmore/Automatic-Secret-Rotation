//! Automatic Secret Rotation CLI
//!
//! This is the main entry point for the CLI application.

mod backends;
mod cli;
mod config;
mod env_updater;
mod rotation;
mod targets;

// Re-export for library usage
pub use config::Config;
pub use rotation::{flag_for_rotation, generate_secret, rotate_secret, scan_for_rotation};

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Parse CLI arguments  
    use clap::Parser;
    let cli = cli::Cli::parse();

    // Execute the command
    cli::execute(cli).await
}
