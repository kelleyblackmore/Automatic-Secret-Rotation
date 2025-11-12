mod config;
mod env_updater;
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

        /// Also update local environment variables (expects env var name to match secret path)
        #[arg(long)]
        update_env: bool,
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

    /// Update a local environment variable with a secret from Vault
    UpdateEnv {
        /// Path to the secret in Vault
        vault_path: String,

        /// Key within the secret data
        #[arg(short, long, default_value = "password")]
        key: String,

        /// Environment variable name to update
        #[arg(short, long)]
        env_var: String,
    },

    /// Generate a new password, store it in Vault, and update local environment variable
    GenPassword {
        /// Path to store the secret in Vault
        vault_path: String,

        /// Key name for the password in Vault
        #[arg(short, long, default_value = "password")]
        key: String,

        /// Environment variable name to update (optional)
        #[arg(short, long)]
        env_var: Option<String>,

        /// Length of the generated password
        #[arg(short, long)]
        length: Option<usize>,
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

        Commands::Auto { path, dry_run, update_env } => {
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

            let env_updater = if update_env {
                Some(env_updater::EnvUpdater::new().context("Failed to create EnvUpdater")?)
            } else {
                None
            };

            for secret_path in &secrets {
                if dry_run {
                    println!("[DRY RUN] Would rotate: {}", secret_path);
                    if update_env {
                        println!("  [DRY RUN] Would update env var based on path");
                    }
                } else {
                    match rotation::rotate_secret(
                        &vault,
                        &config.vault.mount,
                        secret_path,
                        config.rotation.secret_length,
                    )
                    .await
                    {
                        Ok(new_value) => {
                            println!("✓ Rotated: {}", secret_path);
                            
                            // Update environment variable if requested
                            if let Some(ref updater) = env_updater {
                                // Convert path to env var name: myapp/database -> MYAPP_DATABASE
                                let env_var_name = secret_path
                                    .replace('/', "_")
                                    .to_uppercase();
                                
                                match updater.update_env_var(&env_var_name, &new_value) {
                                    Ok(_) => println!("  ✓ Updated env var: {}", env_var_name),
                                    Err(e) => eprintln!("  ✗ Failed to update env var {}: {}", env_var_name, e),
                                }
                            }
                        }
                        Err(e) => {
                            error!("✗ Failed to rotate {}: {}", secret_path, e);
                        }
                    }
                }
            }

            if !dry_run {
                println!("\nRotation complete!");
                if update_env {
                    println!("⚠️  Note: Reload your shell or run 'source ~/.bashrc' for env var changes to take effect");
                }
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

        Commands::UpdateEnv {
            vault_path,
            key,
            env_var,
        } => {
            // Read the secret from Vault
            let secret = vault
                .read_secret(&config.vault.mount, &vault_path)
                .await
                .context("Failed to read secret from Vault")?;

            // Get the specific key value
            let value = secret
                .data
                .get(&key)
                .with_context(|| format!("Key '{}' not found in secret", key))?;

            // Update the environment variable
            let env_updater = env_updater::EnvUpdater::new()
                .context("Failed to create EnvUpdater")?;

            env_updater
                .update_env_var(&env_var, value)
                .with_context(|| format!("Failed to update environment variable {}", env_var))?;

            println!("✓ Updated environment variable '{}' in shell config files", env_var);
            println!("  Value synced from Vault: {}/{} (key: {})", config.vault.mount, vault_path, key);
            println!("\n⚠️  Note: You need to reload your shell or run 'source ~/.bashrc' (or ~/.zshrc) for changes to take effect");
        }

        Commands::GenPassword {
            vault_path,
            key,
            env_var,
            length,
        } => {
            // Generate a new password
            let password_length = length.unwrap_or(config.rotation.secret_length);
            let new_password = rotation::generate_secret(password_length);

            // Prepare secret data
            let mut secret_data = std::collections::HashMap::new();
            secret_data.insert(key.clone(), new_password.clone());

            // Store in Vault
            vault
                .write_secret(&config.vault.mount, &vault_path, secret_data)
                .await
                .context("Failed to write secret to Vault")?;

            println!("✓ Generated new password and stored in Vault");
            println!("  Location: {}/{}", config.vault.mount, vault_path);
            println!("  Key: {}", key);
            println!("  Length: {} characters", password_length);

            // Update local environment variable if specified
            if let Some(env_var_name) = env_var {
                let env_updater = env_updater::EnvUpdater::new()
                    .context("Failed to create EnvUpdater")?;

                env_updater
                    .update_env_var(&env_var_name, &new_password)
                    .with_context(|| format!("Failed to update environment variable {}", env_var_name))?;

                println!("✓ Updated environment variable '{}' in shell config files", env_var_name);
                println!("\n⚠️  Note: You need to reload your shell or run 'source ~/.bashrc' (or ~/.zshrc) for changes to take effect");
            } else {
                println!("\n💡 Tip: Use --env-var to automatically update a local environment variable");
            }
        }
    }

    Ok(())
}
