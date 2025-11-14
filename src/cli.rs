//! CLI parsing and command execution
//!
//! This module handles command-line argument parsing and routes commands to the appropriate handlers.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{error, info};

use crate::backends::Backend;
use crate::config::Config;
use crate::env_updater;
use crate::rotation;
use crate::targets::{Target, TargetInstance};

#[derive(Parser)]
#[command(name = "asr")]
#[command(about = "Automatic secret rotation tool with HashiCorp Vault and AWS Secrets Manager support", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, env = "ROTATOR_CONFIG")]
    pub config: Option<PathBuf>,

    /// Vault address (overrides config file)
    #[arg(long, env = "VAULT_ADDR")]
    pub vault_addr: Option<String>,

    /// Vault token (overrides config file)
    #[arg(long, env = "VAULT_TOKEN")]
    pub vault_token: Option<String>,

    /// Vault mount point (overrides config file)
    #[arg(long, env = "VAULT_MOUNT")]
    pub vault_mount: Option<String>,

    /// Secret backend to use (vault or aws)
    #[arg(long, env = "SECRET_BACKEND")]
    pub backend: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a sample configuration file
    Init {
        /// Output path for the configuration file
        #[arg(short, long, default_value = "rotator-config.toml")]
        output: PathBuf,
    },

    /// Flag a secret for automatic rotation
    Flag {
        /// Path to the secret
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
        /// Path to the secret
        path: String,

    /// Also update target password (database, API, etc.)
    #[arg(long)]
    update_target: bool,

    /// Target type (postgres, api) - defaults to postgres if not specified
    #[arg(long)]
    target_type: Option<String>,

    /// Target username/identifier to update (required if --update-target is set)
    #[arg(long)]
    target_username: Option<String>,
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

        /// Also update target passwords (requires target config and metadata)
        #[arg(long)]
        update_target: bool,
    },

    /// Read a secret
    Read {
        /// Path to the secret
        path: String,
    },

    /// List secrets at a path
    List {
        /// Path to list secrets from
        #[arg(default_value = "")]
        path: String,
    },

    /// Update a local environment variable with a secret
    UpdateEnv {
        /// Path to the secret
        vault_path: String,

        /// Key within the secret data
        #[arg(short, long, default_value = "password")]
        key: String,

        /// Environment variable name to update
        #[arg(short, long)]
        env_var: String,
    },

    /// Generate a new password, store it, and optionally update local environment variable
    GenPassword {
        /// Path to store the secret
        vault_path: String,

        /// Key name for the password
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

/// Execute a CLI command
pub async fn execute(cli: Cli) -> Result<()> {
    // Handle init command separately as it doesn't need backend
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

    // Override backend selection if provided
    if let Some(backend) = cli.backend {
        config.backend = backend.to_lowercase();
    }

    // Override with CLI arguments if provided
    if let Some(addr) = cli.vault_addr {
        if let Some(ref mut vault_config) = config.vault {
            vault_config.address = addr;
        }
    }
    if let Some(token) = cli.vault_token {
        if let Some(ref mut vault_config) = config.vault {
            vault_config.token = token;
        }
    }
    if let Some(mount) = cli.vault_mount {
        if let Some(ref mut vault_config) = config.vault {
            vault_config.mount = mount;
        }
    }

    // Create backend client based on configuration
    let backend = create_backend(&config).await?;

    // Create target if target config is present (support both legacy database and new targets)
    let target = create_target(&config, backend.as_ref()).await?;

    // Execute command
    match cli.command {
        Commands::Init { .. } => unreachable!(), // Handled above

        Commands::Flag { path, period } => {
            rotation::flag_for_rotation(backend.as_ref(), &path, period)
                .await
                .context("Failed to flag secret for rotation")?;
            println!(
                "Successfully flagged {} for rotation every {} months",
                path, period
            );
        }

        Commands::Scan { path } => {
            let secrets = rotation::scan_for_rotation(
                backend.as_ref(),
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

        Commands::Rotate {
            path,
            update_target,
            target_type: _target_type,
            target_username,
        } => {
            if update_target && target_username.is_none() {
                anyhow::bail!("--target-username is required when --update-target is set");
            }

            if update_target && target.is_none() {
                anyhow::bail!("Target configuration not found. Configure [targets.postgres] or [targets.api] section in config file");
            }

            let new_secret = if update_target {
                rotation::rotate_secret_with_target(
                    backend.as_ref(),
                    &path,
                    config.rotation.secret_length,
                    target.as_ref().map(|t| t.as_ref() as &dyn Target),
                    target_username.as_deref(),
                )
                .await
                .context("Failed to rotate secret")?
            } else {
                rotation::rotate_secret(
                    backend.as_ref(),
                    &path,
                    config.rotation.secret_length,
                )
                .await
                .context("Failed to rotate secret")?
            };

            println!("Successfully rotated secret at: {}", path);
            if update_target {
                let target_type_name = target.as_ref().map(|t| t.target_type()).unwrap_or("unknown");
                println!("✓ Updated {} password for user: {}", target_type_name, target_username.as_deref().unwrap_or("unknown"));
            }
            eprintln!(
                "⚠️  WARNING: Secret value will be displayed. Ensure this output is secured."
            );
            println!("New secret value: {}", new_secret);
            eprintln!("⚠️  Please update your application with the new secret and clear your terminal history.");
        }

        Commands::Auto {
            path,
            dry_run,
            update_env,
            update_target,
        } => {
            if update_target && target.is_none() {
                anyhow::bail!("Target configuration not found. Configure [targets.postgres] or [targets.api] section in config file");
            }
            let secrets = rotation::scan_for_rotation(
                backend.as_ref(),
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
                    if update_target {
                        println!("  [DRY RUN] Would update target password (username from metadata)");
                    }
                } else {
                    // Try to get target username from metadata if update_target is enabled
                    let target_username = if update_target {
                        match backend.read_metadata(secret_path).await {
                            Ok(metadata) => metadata.get("target_username").or_else(|| metadata.get("database_username")).cloned(),
                            Err(_) => None,
                        }
                    } else {
                        None
                    };

                    let new_value = if update_target && target_username.is_some() {
                        rotation::rotate_secret_with_target(
                            backend.as_ref(),
                            secret_path,
                            config.rotation.secret_length,
                            target.as_ref().map(|t| t.as_ref() as &dyn Target),
                            target_username.as_deref(),
                        )
                        .await
                    } else {
                        rotation::rotate_secret(
                            backend.as_ref(),
                            secret_path,
                            config.rotation.secret_length,
                        )
                        .await
                    };

                    match new_value {
                        Ok(new_value) => {
                            println!("✓ Rotated: {}", secret_path);

                            // Update target password if requested
                            if update_target && target_username.is_some() {
                                let target_type_name = target.as_ref().map(|t| t.target_type()).unwrap_or("unknown");
                                println!(
                                    "  ✓ Updated {} password for user: {}",
                                    target_type_name,
                                    target_username.as_deref().unwrap_or("unknown")
                                );
                            }

                            // Update environment variable if requested
                            if let Some(ref updater) = env_updater {
                                // Convert path to env var name: myapp/database -> MYAPP_DATABASE
                                let env_var_name = secret_path
                                    .replace('/', "_")
                                    .to_uppercase();

                                match updater.update_env_var(&env_var_name, &new_value) {
                                    Ok(_) => println!("  ✓ Updated env var: {}", env_var_name),
                                    Err(e) => {
                                        eprintln!("  ✗ Failed to update env var {}: {}", env_var_name, e)
                                    }
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
            let secret = backend
                .read_secret(&path)
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
            let secrets = backend
                .list_secrets(&path)
                .await
                .context("Failed to list secrets")?;
            if secrets.is_empty() {
                println!("No secrets found at path: {}", if path.is_empty() { "/" } else { &path });
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
            // Read the secret from backend
            let secret = backend
                .read_secret(&vault_path)
                .await
                .context("Failed to read secret")?;

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
            println!("  Value synced from {}: {} (key: {})", backend.backend_type(), vault_path, key);
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

            // Store in backend
            backend
                .write_secret(&vault_path, secret_data)
                .await
                .context("Failed to write secret")?;

            println!("✓ Generated new password and stored in {}", backend.backend_type());
            println!("  Location: {}", vault_path);
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

/// Create a target instance based on configuration
/// Supports both legacy [database] config and new [targets] config
async fn create_target(
    config: &Config,
    backend: &dyn crate::backends::SecretBackend,
) -> Result<Option<TargetInstance>> {
    // Check for new targets config first
    if let Some(ref targets_config) = config.targets {
        // Try PostgreSQL target
        if let Some(ref postgres_config) = targets_config.postgres {
            return Ok(Some(create_postgres_target(postgres_config, backend).await?));
        }
        
        // Try API target
        if let Some(ref api_config) = targets_config.api {
            return Ok(Some(create_api_target(api_config).await?));
        }
    }
    
    // Fall back to legacy database config for backward compatibility
    if let Some(ref db_config) = config.database {
        return Ok(Some(create_postgres_target(db_config, backend).await?));
    }
    
    Ok(None)
}

/// Create a PostgreSQL target instance
async fn create_postgres_target(
    config: &crate::config::PostgresTargetConfig,
    backend: &dyn crate::backends::SecretBackend,
) -> Result<TargetInstance> {
    // Get admin password from secret backend or direct config
    let admin_password = if let Some(ref password_path) = config.password_path {
        // Read from secret backend
        let secret = backend
            .read_secret(password_path)
            .await
            .context("Failed to read admin password from secret backend")?;
        
        // Try to find password key
        secret
            .data
            .values()
            .next()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No password found in secret at {}", password_path))?
    } else if let Some(ref password) = config.password {
        password.clone()
    } else {
        anyhow::bail!("PostgreSQL password not configured. Set password_path or password in config");
    };

    let target = crate::targets::PostgresTarget::new(config, &admin_password)
        .await
        .context("Failed to create PostgreSQL target")?;

    Ok(Box::new(target))
}

/// Create an API target instance
async fn create_api_target(
    config: &crate::config::ApiTargetConfig,
) -> Result<TargetInstance> {
    let target = crate::targets::ApiTarget::new(config)
        .await
        .context("Failed to create API target")?;

    Ok(Box::new(target))
}

/// Create a backend instance based on configuration
async fn create_backend(config: &Config) -> Result<Backend> {
    match config.backend.as_str() {
        "aws" => {
            let aws_config = config.aws.as_ref().ok_or_else(|| {
                anyhow::anyhow!("AWS configuration not found. Set AWS_REGION or configure [aws] section")
            })?;
            let aws_client = crate::backends::AwsSecretsClient::new(Some(aws_config.region.clone()))
                .await
                .context("Failed to create AWS Secrets Manager client")?;
            Ok(Box::new(aws_client))
        }
        "file" => {
            let file_config = config.file.as_ref().ok_or_else(|| {
                anyhow::anyhow!("File configuration not found. Set ASR_FILE_DIR or configure [file] section")
            })?;
            let file_backend = crate::backends::FileBackend::new(&file_config.directory)
                .context("Failed to create file backend")?;
            Ok(Box::new(file_backend))
        }
        "vault" => {
            let vault_config = config.vault.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Vault configuration not found. Set VAULT_ADDR/VAULT_TOKEN or configure [vault] section")
            })?;
            let vault_client = crate::backends::VaultClient::new(
                vault_config.address.clone(),
                vault_config.token.clone(),
            )
            .context("Failed to create Vault client")?;
            Ok(Box::new(crate::backends::VaultBackend::new(
                vault_client,
                vault_config.mount.clone(),
            )))
        }
        _ => {
            let vault_config = config.vault.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Vault configuration not found. Set VAULT_ADDR/VAULT_TOKEN or configure [vault] section")
            })?;
            let vault_client = crate::backends::VaultClient::new(
                vault_config.address.clone(),
                vault_config.token.clone(),
            )
            .context("Failed to create Vault client")?;
            Ok(Box::new(crate::backends::VaultBackend::new(
                vault_client,
                vault_config.mount.clone(),
            )))
        }
    }
}

