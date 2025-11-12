mod config;
mod rotation;
mod vault;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use vault::VaultClient;

#[derive(Parser)]
#[command(name = "asr")]
#[command(about = "Automatic secret rotation tool with HashiCorp Vault integration", long_about = None)]
#[command(version)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, env = "ROTATOR_CONFIG")]
    config: Option<PathBuf>,

    /// Vault address (overrides config file)
    #[arg(long, env = "VAULT_ADDR")]
    vault_addr: Option<String>,

    /// Vault token (overrides config file)
    #[arg(long, env = "VAULT_TOKEN")]
    vault_token: Option<String>,

    /// Vault mount point (overrides config file)
    #[arg(long, env = "VAULT_MOUNT")]
    vault_mount: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a sample configuration file
    Init {
        /// Output path for the configuration file
        #[arg(short, long, default_value = "rotator-config.toml")]
        output: PathBuf,
    },

    /// Flag a secret for automatic rotation
    Flag {
        /// Path to the secret in Vault
        path: String,

        /// Rotation period in months
        #[arg(short, long, default_value = "6")]
        period: u32,
    },

    /// Scan for secrets that need rotation
    Scan {
        /// Base path to scan (leave empty for root)
        #[arg(default_value = "")]
        path: String,
    },

    /// Rotate a specific secret
    Rotate {
        /// Path to the secret in Vault
        path: String,
    },

    /// Automatically rotate all secrets that are due for rotation
    Auto {
        /// Base path to scan (leave empty for root)
        #[arg(default_value = "")]
        path: String,

        /// Dry run - only show what would be rotated
        #[arg(long)]
        dry_run: bool,
    },

    /// Read a secret from Vault
    Read {
        /// Path to the secret in Vault
        path: String,
    },

    /// List secrets at a path
    List {
        /// Path to list secrets from
        #[arg(default_value = "")]
        path: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Handle init command separately as it doesn't need Vault
    if let Commands::Init { output } = cli.command {
        Config::create_sample(&output)
            .with_context(|| format!("Failed to create sample config at {:?}", output))?;
        info!("Sample configuration created at {:?}", output);
        return Ok(());
    }

    // Load configuration
    let mut config = if let Some(config_path) = cli.config {
        Config::from_file(&config_path)
            .with_context(|| format!("Failed to load config from {:?}", config_path))?
    } else {
        Config::from_env().context("Failed to load config from environment")?
    };

    // Override with CLI arguments if provided
    if let Some(addr) = cli.vault_addr {
        config.vault.address = addr;
    }
    if let Some(token) = cli.vault_token {
        config.vault.token = token;
    }
    if let Some(mount) = cli.vault_mount {
        config.vault.mount = mount;
    }

    // Create Vault client
    let vault = VaultClient::new(config.vault.address.clone(), config.vault.token.clone())
        .context("Failed to create Vault client")?;

    // Execute command
    match cli.command {
        Commands::Init { .. } => unreachable!(), // Handled above

        Commands::Flag { path, period } => {
            rotation::flag_for_rotation(&vault, &config.vault.mount, &path, period)
                .await
                .context("Failed to flag secret for rotation")?;
            println!(
                "Successfully flagged {} for rotation every {} months",
                path, period
            );
        }

        Commands::Scan { path } => {
            let secrets = rotation::scan_for_rotation(
                &vault,
                &config.vault.mount,
                &path,
                config.rotation.period_months,
            )
            .await
            .context("Failed to scan for secrets needing rotation")?;

            if secrets.is_empty() {
                println!("No secrets need rotation at this time");
            } else {
                println!("Secrets needing rotation:");
                for secret in secrets {
                    println!("  - {}", secret);
                }
            }
        }

        Commands::Rotate { path } => {
            let new_secret = rotation::rotate_secret(
                &vault,
                &config.vault.mount,
                &path,
                config.rotation.secret_length,
            )
            .await
            .context("Failed to rotate secret")?;
            println!("Successfully rotated secret at: {}", path);
            eprintln!(
                "⚠️  WARNING: Secret value will be displayed. Ensure this output is secured."
            );
            println!("New secret value: {}", new_secret);
            eprintln!("⚠️  Please update your application with the new secret and clear your terminal history.");
        }

        Commands::Auto { path, dry_run } => {
            let secrets = rotation::scan_for_rotation(
                &vault,
                &config.vault.mount,
                &path,
                config.rotation.period_months,
            )
            .await
            .context("Failed to scan for secrets needing rotation")?;

            if secrets.is_empty() {
                println!("No secrets need rotation at this time");
                return Ok(());
            }

            println!("Found {} secret(s) needing rotation", secrets.len());

            for secret_path in &secrets {
                if dry_run {
                    println!("[DRY RUN] Would rotate: {}", secret_path);
                } else {
                    match rotation::rotate_secret(
                        &vault,
                        &config.vault.mount,
                        secret_path,
                        config.rotation.secret_length,
                    )
                    .await
                    {
                        Ok(_) => println!("✓ Rotated: {}", secret_path),
                        Err(e) => {
                            error!("✗ Failed to rotate {}: {}", secret_path, e);
                        }
                    }
                }
            }

            if !dry_run {
                println!("\nRotation complete!");
            }
        }

        Commands::Read { path } => {
            let secret = vault
                .read_secret(&config.vault.mount, &path)
                .await
                .context("Failed to read secret")?;
            eprintln!(
                "⚠️  WARNING: Secret values will be displayed. Ensure this output is secured."
            );
            println!("Secret data:");
            for (key, value) in secret.data {
                println!("  {}: {}", key, value);
            }
            eprintln!("⚠️  Please clear your terminal history after viewing.");
        }

        Commands::List { path } => {
            let secrets = vault
                .list_secrets(&config.vault.mount, &path)
                .await
                .context("Failed to list secrets")?;
            if secrets.is_empty() {
                println!("No secrets found at path: {}", path);
            } else {
                println!("Secrets at {}:", if path.is_empty() { "/" } else { &path });
                for secret in secrets {
                    println!("  - {}", secret);
                }
            }
        }
    }

    Ok(())
}
