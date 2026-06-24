//! CLI parsing and command execution

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{error, info};

use crate::audit::{AuditEvent, AuditLogger};
use crate::backends::Backend;
use crate::config::{Config, TargetEntry, TargetsSpec};
use crate::env_updater;
use crate::notification::NotificationClient;
use crate::rotation;
use crate::targets::TargetInstance;

#[derive(Parser)]
#[command(name = "asr")]
#[command(
    about = "Automatic secret rotation — Vault, AWS, Azure, GCP, OCP, and more",
    long_about = None
)]
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

    /// Secret backend to use (vault, aws, file, azure, gcp, ocp)
    #[arg(long, env = "SECRET_BACKEND")]
    pub backend: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a sample configuration file
    Init {
        #[arg(short, long, default_value = "rotator-config.toml")]
        output: PathBuf,
    },

    /// Flag a secret for automatic rotation
    Flag {
        path: String,
        #[arg(short, long, default_value = "6")]
        period: u32,
    },

    /// Scan for secrets that need rotation
    Scan {
        #[arg(default_value = "")]
        path: String,
    },

    /// Rotate a specific secret
    Rotate {
        path: String,
        /// Also update all configured target systems (databases, APIs)
        #[arg(long)]
        update_target: bool,
        /// Target username/identifier to update (required when --update-target is set)
        #[arg(long)]
        target_username: Option<String>,
    },

    /// Automatically rotate all secrets that are due for rotation
    Auto {
        #[arg(default_value = "")]
        path: String,
        /// Dry run — show what would be rotated without making changes
        #[arg(long)]
        dry_run: bool,
        /// Update local environment variables with new secret values
        #[arg(long)]
        update_env: bool,
        /// Update target systems (databases, APIs) with new passwords
        #[arg(long)]
        update_target: bool,
    },

    /// Read a secret
    Read { path: String },

    /// List secrets at a path
    List {
        #[arg(default_value = "")]
        path: String,
    },

    /// Sync a secret from the backend into a local environment variable
    UpdateEnv {
        vault_path: String,
        #[arg(short, long, default_value = "password")]
        key: String,
        #[arg(short, long)]
        env_var: String,
    },

    /// Generate a new random password, store it in the backend, and optionally sync to env
    GenPassword {
        vault_path: String,
        #[arg(short, long, default_value = "password")]
        key: String,
        #[arg(short, long)]
        env_var: Option<String>,
        #[arg(short, long)]
        length: Option<usize>,
    },

    /// Store a secret into the macOS Keychain (macOS only)
    #[cfg(target_os = "macos")]
    UpdateKeychain {
        /// Secret path to read from the backend
        path: String,
        /// Key within the secret data (default: "password")
        #[arg(short, long, default_value = "password")]
        key: String,
        /// Keychain service name (defaults to "asr/<path>")
        #[arg(long)]
        service: Option<String>,
        /// Keychain account name (defaults to the secret key)
        #[arg(long)]
        account: Option<String>,
    },
}

pub async fn execute(cli: Cli) -> Result<()> {
    if let Commands::Init { output } = cli.command {
        Config::create_sample(&output)
            .with_context(|| format!("Failed to create sample config at {:?}", output))?;
        info!("Sample configuration created at {:?}", output);
        return Ok(());
    }

    let mut config = if let Some(config_path) = cli.config {
        Config::from_file(&config_path)
            .with_context(|| format!("Failed to load config from {:?}", config_path))?
    } else {
        Config::from_env().context("Failed to load config from environment")?
    };

    // CLI overrides
    if let Some(backend) = cli.backend {
        config.backend = backend.to_lowercase();
    }
    if let Some(addr) = cli.vault_addr {
        if let Some(ref mut v) = config.vault {
            v.address = addr;
        }
    }
    if let Some(token) = cli.vault_token {
        if let Some(ref mut v) = config.vault {
            v.token = token;
        }
    }
    if let Some(mount) = cli.vault_mount {
        if let Some(ref mut v) = config.vault {
            v.mount = mount;
        }
    }

    let backend = create_backend(&config).await?;
    let targets = create_targets(&config, backend.as_ref()).await?;
    let audit = AuditLogger::new(&config.audit);
    let notifier = NotificationClient::new(&config.notification);

    match cli.command {
        Commands::Init { .. } => unreachable!(),

        Commands::Flag { path, period } => {
            rotation::flag_for_rotation(backend.as_ref(), &path, period)
                .await
                .context("Failed to flag secret for rotation")?;

            audit.log(&AuditEvent::new("flagged", &path, backend.backend_type()));
            notifier
                .notify_flag(&path, backend.backend_type(), period)
                .await
                .ok();

            println!("Flagged {} for rotation every {} months", path, period);
        }

        Commands::Scan { path } => {
            let secrets =
                rotation::scan_for_rotation(backend.as_ref(), &path, config.rotation.period_months)
                    .await
                    .context("Failed to scan for secrets needing rotation")?;

            let scan_path = if path.is_empty() { "/" } else { &path };
            audit.log(&AuditEvent::new(
                "scanned",
                scan_path,
                backend.backend_type(),
            ));

            notifier
                .notify_scan(&path, backend.backend_type(), secrets.len())
                .await
                .ok();

            if secrets.is_empty() {
                println!("No secrets need rotation at this time");
            } else {
                println!("Secrets needing rotation:");
                for s in secrets {
                    println!("  - {}", s);
                }
            }
        }

        Commands::Rotate {
            path,
            update_target,
            target_username,
        } => {
            if update_target && target_username.is_none() {
                anyhow::bail!("--target-username is required when --update-target is set");
            }
            if update_target && targets.is_empty() {
                anyhow::bail!(
                    "No target configured. Add a [targets.postgres] / [targets.api] section \
                     or a [[targets]] array to your config."
                );
            }

            let start = std::time::Instant::now();

            let result = if update_target {
                rotation::rotate_secret_with_targets(
                    backend.as_ref(),
                    &path,
                    config.rotation.secret_length,
                    &targets,
                    target_username.as_deref(),
                )
                .await
                .context("Failed to rotate secret")
            } else {
                rotation::rotate_secret(backend.as_ref(), &path, config.rotation.secret_length)
                    .await
                    .context("Failed to rotate secret")
            };

            let duration_ms = start.elapsed().as_millis() as u64;

            match result {
                Ok(new_secret) => {
                    audit.log(
                        &AuditEvent::new("rotated", &path, backend.backend_type())
                            .with_duration(duration_ms),
                    );
                    notifier
                        .notify_rotate(&path, backend.backend_type(), "success", None)
                        .await
                        .ok();

                    println!("Rotated: {}", path);
                    if update_target {
                        for t in &targets {
                            println!(
                                "  Updated {} password for: {}",
                                t.target_type(),
                                target_username.as_deref().unwrap_or("(unknown)")
                            );
                        }
                    }
                    eprintln!(
                        "WARNING: Secret value displayed below. Secure or clear your terminal history."
                    );
                    println!("New secret: {}", new_secret);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    audit.log(
                        &AuditEvent::new("rotated", &path, backend.backend_type())
                            .with_duration(duration_ms)
                            .with_error(&err_str),
                    );
                    notifier
                        .notify_rotate(&path, backend.backend_type(), "failed", Some(&err_str))
                        .await
                        .ok();
                    return Err(e);
                }
            }
        }

        Commands::Auto {
            path,
            dry_run,
            update_env,
            update_target,
        } => {
            if update_target && targets.is_empty() {
                anyhow::bail!(
                    "No target configured. Add a [targets.postgres] / [targets.api] section \
                     or a [[targets]] array to your config."
                );
            }

            let secrets =
                rotation::scan_for_rotation(backend.as_ref(), &path, config.rotation.period_months)
                    .await
                    .context("Failed to scan for secrets needing rotation")?;

            notifier
                .notify_scan(&path, backend.backend_type(), secrets.len())
                .await
                .ok();

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
                        let env_var = secret_path.replace('/', "_").to_uppercase();
                        println!("  [DRY RUN] Would update env var: {}", env_var);
                    }
                    if update_target {
                        println!("  [DRY RUN] Would update {} target(s)", targets.len());
                    }
                    continue;
                }

                let target_username = if update_target {
                    backend.read_metadata(secret_path).await.ok().and_then(|m| {
                        m.get("target_username")
                            .or_else(|| m.get("database_username"))
                            .cloned()
                    })
                } else {
                    None
                };

                let start = std::time::Instant::now();

                let result = if update_target && target_username.is_some() {
                    rotation::rotate_secret_with_targets(
                        backend.as_ref(),
                        secret_path,
                        config.rotation.secret_length,
                        &targets,
                        target_username.as_deref(),
                    )
                    .await
                    .with_context(|| format!("Failed to rotate: {}", secret_path))
                } else {
                    rotation::rotate_secret(
                        backend.as_ref(),
                        secret_path,
                        config.rotation.secret_length,
                    )
                    .await
                    .with_context(|| format!("Failed to rotate: {}", secret_path))
                };

                let duration_ms = start.elapsed().as_millis() as u64;

                match result {
                    Ok(new_value) => {
                        audit.log(
                            &AuditEvent::new("rotated", secret_path, backend.backend_type())
                                .with_duration(duration_ms),
                        );
                        notifier
                            .notify_rotate(secret_path, backend.backend_type(), "success", None)
                            .await
                            .ok();

                        println!("Rotated: {}", secret_path);

                        if update_target && target_username.is_some() {
                            for t in &targets {
                                println!(
                                    "  Updated {} password for: {}",
                                    t.target_type(),
                                    target_username.as_deref().unwrap_or("(unknown)")
                                );
                            }
                        }

                        if let Some(ref updater) = env_updater {
                            let env_var = secret_path.replace('/', "_").to_uppercase();
                            match updater.update_env_var(&env_var, &new_value) {
                                Ok(_) => println!("  Updated env var: {}", env_var),
                                Err(e) => {
                                    eprintln!("  Failed to update env var {}: {}", env_var, e)
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        error!("Failed to rotate {}: {}", secret_path, err_str);
                        audit.log(
                            &AuditEvent::new("rotated", secret_path, backend.backend_type())
                                .with_duration(duration_ms)
                                .with_error(&err_str),
                        );
                        notifier
                            .notify_rotate(
                                secret_path,
                                backend.backend_type(),
                                "failed",
                                Some(&err_str),
                            )
                            .await
                            .ok();
                    }
                }
            }

            if !dry_run {
                println!("\nRotation complete!");
                if update_env {
                    println!(
                        "  Note: Run 'source ~/.bashrc' (or ~/.zshrc) to apply env var changes"
                    );
                }
            }
        }

        Commands::Read { path } => {
            let secret = backend
                .read_secret(&path)
                .await
                .context("Failed to read secret")?;
            eprintln!("WARNING: Secret values will be displayed below.");
            println!("Secret data:");
            for (k, v) in secret.data {
                println!("  {}: {}", k, v);
            }
            eprintln!("  Please clear your terminal history after viewing.");
        }

        Commands::List { path } => {
            let secrets = backend
                .list_secrets(&path)
                .await
                .context("Failed to list secrets")?;
            let display_path = if path.is_empty() { "/" } else { &path };
            if secrets.is_empty() {
                println!("No secrets found at: {}", display_path);
            } else {
                println!("Secrets at {}:", display_path);
                for s in secrets {
                    println!("  - {}", s);
                }
            }
        }

        Commands::UpdateEnv {
            vault_path,
            key,
            env_var,
        } => {
            let secret = backend
                .read_secret(&vault_path)
                .await
                .context("Failed to read secret")?;

            let value = secret
                .data
                .get(&key)
                .with_context(|| format!("Key '{}' not found in secret", key))?;

            let updater = env_updater::EnvUpdater::new().context("Failed to create EnvUpdater")?;
            updater
                .update_env_var(&env_var, value)
                .with_context(|| format!("Failed to update environment variable {}", env_var))?;

            println!("Updated '{}' in shell config files", env_var);
            println!(
                "  Synced from {}: {} (key: {})",
                backend.backend_type(),
                vault_path,
                key
            );
            println!("\nRun 'source ~/.bashrc' (or ~/.zshrc) for changes to take effect");
        }

        Commands::GenPassword {
            vault_path,
            key,
            env_var,
            length,
        } => {
            let password_length = length.unwrap_or(config.rotation.secret_length);
            let new_password = rotation::generate_secret(password_length);

            let mut secret_data = std::collections::HashMap::new();
            secret_data.insert(key.clone(), new_password.clone());

            backend
                .write_secret(&vault_path, secret_data)
                .await
                .context("Failed to write secret")?;

            println!(
                "Generated password stored in {} at {}",
                backend.backend_type(),
                vault_path
            );
            println!("  Key: {}  Length: {} chars", key, password_length);

            if let Some(env_var_name) = env_var {
                let updater =
                    env_updater::EnvUpdater::new().context("Failed to create EnvUpdater")?;
                updater
                    .update_env_var(&env_var_name, &new_password)
                    .with_context(|| {
                        format!("Failed to update environment variable {}", env_var_name)
                    })?;
                println!("Updated env var '{}' in shell config files", env_var_name);
                println!("Run 'source ~/.bashrc' (or ~/.zshrc) to apply changes");
            }
        }

        #[cfg(target_os = "macos")]
        Commands::UpdateKeychain {
            path,
            key,
            service,
            account,
        } => {
            #[cfg(not(feature = "keychain"))]
            {
                anyhow::bail!(
                    "The update-keychain command requires building with `--features keychain`. \
                     Install via: cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation --features keychain"
                );
                // Suppress unused variable warnings when feature is disabled
                let _ = (path, key, service, account, backend);
            }

            #[cfg(feature = "keychain")]
            {
                let secret = backend
                    .read_secret(&path)
                    .await
                    .context("Failed to read secret from backend")?;

                let value = secret
                    .data
                    .get(&key)
                    .with_context(|| format!("Key '{}' not found in secret", key))?;

                let svc = service.unwrap_or_else(|| format!("asr/{}", path));
                let acct = account.unwrap_or_else(|| key.clone());

                let entry =
                    keyring::Entry::new(&svc, &acct).context("Failed to create Keychain entry")?;

                entry
                    .set_password(value)
                    .context("Failed to store secret in macOS Keychain")?;

                println!(
                    "Stored secret in macOS Keychain: service='{}' account='{}'",
                    svc, acct
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Target construction helpers
// ---------------------------------------------------------------------------

async fn resolve_password(
    backend: &dyn crate::backends::SecretBackend,
    password_path: Option<&str>,
    password: Option<&str>,
    kind: &str,
) -> Result<String> {
    if let Some(path) = password_path {
        let secret = backend
            .read_secret(path)
            .await
            .context("Failed to read admin password from backend")?;
        secret
            .data
            .values()
            .next()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No password found in secret at {}", path))
    } else if let Some(pw) = password {
        Ok(pw.to_string())
    } else {
        anyhow::bail!(
            "{} target requires password_path or password in config",
            kind
        )
    }
}

/// Build the full list of targets from config (supports both old and new forms).
async fn create_targets(
    config: &Config,
    backend: &dyn crate::backends::SecretBackend,
) -> Result<Vec<TargetInstance>> {
    let mut result: Vec<TargetInstance> = Vec::new();

    match config.targets.as_ref() {
        Some(TargetsSpec::List(entries)) => {
            for entry in entries {
                result.push(create_target_from_entry(entry, backend).await?);
            }
        }
        Some(TargetsSpec::Named(named)) => {
            if let Some(ref pg) = named.postgres {
                result.push(create_postgres_target(pg, backend).await?);
            }
            if let Some(ref api) = named.api {
                result.push(create_api_target(api).await?);
            }
            if let Some(ref mysql) = named.mysql {
                result.push(create_mysql_target(mysql, backend).await?);
            }
            if let Some(ref gitlab) = named.gitlab {
                result.push(create_gitlab_target(gitlab).await?);
            }
            if let Some(ref github) = named.github {
                result.push(create_github_target(github).await?);
            }
        }
        None => {
            // Fall back to legacy [database] config
            if let Some(ref db) = config.database {
                result.push(create_postgres_target(db, backend).await?);
            }
        }
    }

    Ok(result)
}

async fn create_target_from_entry(
    entry: &TargetEntry,
    backend: &dyn crate::backends::SecretBackend,
) -> Result<TargetInstance> {
    match entry {
        TargetEntry::Postgres(pg) => create_postgres_target(pg, backend).await,
        TargetEntry::Api(api) => create_api_target(api).await,
        TargetEntry::Mysql(mysql) => create_mysql_target(mysql, backend).await,
        TargetEntry::Gitlab(gitlab) => create_gitlab_target(gitlab).await,
        TargetEntry::Github(github) => create_github_target(github).await,
    }
}

async fn create_postgres_target(
    config: &crate::config::PostgresTargetConfig,
    backend: &dyn crate::backends::SecretBackend,
) -> Result<TargetInstance> {
    let admin_password = resolve_password(
        backend,
        config.password_path.as_deref(),
        config.password.as_deref(),
        "PostgreSQL",
    )
    .await?;

    let target = crate::targets::PostgresTarget::new(config, &admin_password)
        .await
        .context("Failed to create PostgreSQL target")?;
    Ok(Box::new(target))
}

async fn create_api_target(config: &crate::config::ApiTargetConfig) -> Result<TargetInstance> {
    let target = crate::targets::ApiTarget::new(config)
        .await
        .context("Failed to create API target")?;
    Ok(Box::new(target))
}

#[cfg(feature = "mysql")]
async fn create_mysql_target(
    config: &crate::config::MysqlTargetConfig,
    backend: &dyn crate::backends::SecretBackend,
) -> Result<TargetInstance> {
    let admin_password = resolve_password(
        backend,
        config.password_path.as_deref(),
        config.password.as_deref(),
        "MySQL",
    )
    .await?;

    let target = crate::targets::MysqlTarget::new(config, &admin_password)
        .await
        .context("Failed to create MySQL target")?;
    Ok(Box::new(target))
}

#[cfg(not(feature = "mysql"))]
async fn create_mysql_target(
    _config: &crate::config::MysqlTargetConfig,
    _backend: &dyn crate::backends::SecretBackend,
) -> Result<TargetInstance> {
    anyhow::bail!(
        "MySQL target support requires building with `--features mysql`.\n\
         Rebuild with: cargo install --git ... --features mysql"
    )
}

#[cfg(feature = "gitlab")]
async fn create_gitlab_target(
    config: &crate::config::GitLabTargetConfig,
) -> Result<TargetInstance> {
    let target = crate::targets::GitLabTarget::new(config)
        .await
        .context("Failed to create GitLab target")?;
    Ok(Box::new(target))
}

#[cfg(not(feature = "gitlab"))]
async fn create_gitlab_target(
    _config: &crate::config::GitLabTargetConfig,
) -> Result<TargetInstance> {
    anyhow::bail!(
        "GitLab target support requires building with `--features gitlab`.\n\
         Rebuild with: cargo install --git ... --features gitlab"
    )
}

#[cfg(feature = "github")]
async fn create_github_target(
    config: &crate::config::GitHubTargetConfig,
) -> Result<TargetInstance> {
    let target = crate::targets::GitHubTarget::new(config)
        .await
        .context("Failed to create GitHub target")?;
    Ok(Box::new(target))
}

#[cfg(not(feature = "github"))]
async fn create_github_target(
    _config: &crate::config::GitHubTargetConfig,
) -> Result<TargetInstance> {
    anyhow::bail!(
        "GitHub target support requires building with `--features github`.\n\
         Rebuild with: cargo install --git ... --features github"
    )
}

// ---------------------------------------------------------------------------
// Backend construction
// ---------------------------------------------------------------------------

async fn create_backend(config: &Config) -> Result<Backend> {
    match config.backend.as_str() {
        "vault" => {
            let vault_config = config.vault.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Vault config not found. Set VAULT_ADDR/VAULT_TOKEN or add [vault] section."
                )
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

        "aws" => {
            let aws_config = config.aws.as_ref().ok_or_else(|| {
                anyhow::anyhow!("AWS config not found. Set AWS_REGION or add [aws] section.")
            })?;
            let client = crate::backends::AwsSecretsClient::new(Some(aws_config.region.clone()))
                .await
                .context("Failed to create AWS Secrets Manager client")?;
            Ok(Box::new(client))
        }

        "file" => {
            let file_config = config.file.as_ref().ok_or_else(|| {
                anyhow::anyhow!("File config not found. Set ASR_FILE_DIR or add [file] section.")
            })?;
            let backend = crate::backends::FileBackend::new(&file_config.directory)
                .context("Failed to create file backend")?;
            Ok(Box::new(backend))
        }

        "azure" => create_azure_backend(config).await,

        "gcp" => create_gcp_backend(config).await,

        "ocp" | "k8s" | "kubernetes" => create_ocp_backend(config).await,

        other => {
            anyhow::bail!(
                "Unknown backend: '{}'. Supported: vault, aws, file, azure, gcp, ocp",
                other
            )
        }
    }
}

#[cfg(feature = "azure")]
async fn create_azure_backend(config: &Config) -> Result<Backend> {
    let azure_config = config.azure.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Azure config not found. Set AZURE_VAULT_URL or add [azure] section.")
    })?;
    let backend = crate::backends::AzureKeyVaultBackend::new(azure_config)
        .await
        .context("Failed to create Azure Key Vault backend")?;
    Ok(Box::new(backend))
}

#[cfg(not(feature = "azure"))]
async fn create_azure_backend(_config: &Config) -> Result<Backend> {
    anyhow::bail!(
        "Azure Key Vault backend requires building with `--features azure`.\n\
         Rebuild with: cargo install --git ... --features azure"
    )
}

#[cfg(feature = "gcp")]
async fn create_gcp_backend(config: &Config) -> Result<Backend> {
    let gcp_config = config.gcp.as_ref().ok_or_else(|| {
        anyhow::anyhow!("GCP config not found. Set GCP_PROJECT_ID or add [gcp] section.")
    })?;
    let backend = crate::backends::GcpSecretManagerBackend::new(gcp_config)
        .await
        .context("Failed to create GCP Secret Manager backend")?;
    Ok(Box::new(backend))
}

#[cfg(not(feature = "gcp"))]
async fn create_gcp_backend(_config: &Config) -> Result<Backend> {
    anyhow::bail!(
        "GCP Secret Manager backend requires building with `--features gcp`.\n\
         Rebuild with: cargo install --git ... --features gcp"
    )
}

#[cfg(feature = "ocp")]
async fn create_ocp_backend(config: &Config) -> Result<Backend> {
    let ocp_config = config.ocp.as_ref().ok_or_else(|| {
        anyhow::anyhow!("OCP config not found. Set OCP_NAMESPACE or add [ocp] section.")
    })?;
    let backend = crate::backends::OcpBackend::new(ocp_config)
        .await
        .context("Failed to create OpenShift/Kubernetes backend")?;
    Ok(Box::new(backend))
}

#[cfg(not(feature = "ocp"))]
async fn create_ocp_backend(_config: &Config) -> Result<Backend> {
    anyhow::bail!(
        "OpenShift/Kubernetes backend requires building with `--features ocp`.\n\
         Rebuild with: cargo install --git ... --features ocp"
    )
}
