use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use super::secret_backend::{SecretBackend, SecretData};

/// File-based backend for storing secrets in local flat files
///
/// Format: Each secret is stored in a file with key:value pairs, one per line
/// Example:
///   password:mysecret123
///   username:admin
///
/// Metadata is stored in a separate .meta file alongside the secret file
pub struct FileBackend {
    base_dir: PathBuf,
}

impl FileBackend {
    /// Create a new file backend
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let mut base_dir = base_dir.as_ref().to_path_buf();

        // Expand tilde (~) to home directory
        if let Some(base_dir_str) = base_dir.to_str() {
            if base_dir_str.starts_with("~") {
                let home = std::env::var("HOME").context("HOME environment variable not set")?;
                base_dir = base_dir_str.replacen("~", &home, 1).into();
            }
        }

        // Create base directory if it doesn't exist
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir)
                .with_context(|| format!("Failed to create base directory: {:?}", base_dir))?;
            // Restrict to owner only on Unix (0o700)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&base_dir, fs::Permissions::from_mode(0o700))
                    .with_context(|| format!("Failed to set permissions on {:?}", base_dir))?;
            }
            info!("Created base directory: {:?}", base_dir);
        }

        Ok(Self { base_dir })
    }

    /// Convert a secret path to a file path
    /// Example: "myapp/database" -> base_dir/myapp/database
    ///
    /// The path must stay inside the base directory: absolute paths, `..`
    /// and drive prefixes are refused, because `join` would otherwise let a
    /// path like `../../etc/x` or `/etc/x` read or overwrite any file the
    /// user can reach.
    fn path_to_file(&self, path: &str) -> Result<PathBuf> {
        use std::path::Component;
        let relative = Path::new(path);
        let escapes = relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
        if escapes {
            anyhow::bail!(
                "Invalid secret path {:?}: it must be relative and stay inside {:?}",
                path,
                self.base_dir
            );
        }
        if relative.extension().is_some_and(|e| e == "meta") {
            anyhow::bail!(
                "Invalid secret path {:?}: the .meta extension is reserved for metadata",
                path
            );
        }
        Ok(self.base_dir.join(relative))
    }

    /// Get metadata file path for a secret: the secret's file name plus
    /// `.meta`. (It used to replace the extension, so `db.v1` and `db.v2`
    /// shared `db.meta` and overwrote each other's rotation schedule.)
    fn metadata_path(&self, path: &str) -> Result<PathBuf> {
        let mut meta = self.path_to_file(path)?.into_os_string();
        meta.push(".meta");
        Ok(PathBuf::from(meta))
    }

    /// Where metadata lived before 0.7.1, read as a fallback so existing
    /// rotation schedules for dotted names keep working until next written.
    fn legacy_metadata_path(&self, path: &str) -> Result<PathBuf> {
        Ok(self.path_to_file(path)?.with_extension("meta"))
    }

    /// Parse a key:value line from the secret file
    pub fn parse_line(line: &str) -> Option<(String, String)> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
        } else {
            None
        }
    }

    /// Format a key:value pair for writing
    fn format_line(key: &str, value: &str) -> String {
        format!("{}:{}\n", key, value)
    }

    /// Ensure parent directories exist for a path
    fn ensure_parent_dir(&self, file_path: &Path) -> Result<()> {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directory: {:?}", parent))?;
            // Restrict directory to owner only on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                    .with_context(|| format!("Failed to set permissions on {:?}", parent))?;
            }
        }
        Ok(())
    }

    /// Write content to a file readable only by its owner (0o600 on Unix).
    ///
    /// The content goes to a temporary file in the same directory that is
    /// created with mode 0o600, then renamed over the target. The secret is
    /// therefore never in a file other users can read, not even briefly, and
    /// a crash mid-write leaves the old secret intact rather than half a file.
    fn write_restricted(path: &Path, content: &str) -> Result<()> {
        use std::io::Write;

        let file_name = path
            .file_name()
            .with_context(|| format!("Invalid secret file path: {:?}", path))?
            .to_string_lossy();
        let tmp_path = path.with_file_name(format!(
            ".{}.tmp-{}-{}",
            file_name,
            std::process::id(),
            rand::random::<u32>()
        ));

        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);

        let result = options
            .open(&tmp_path)
            .and_then(|mut f| {
                f.write_all(content.as_bytes())?;
                f.sync_all()
            })
            .and_then(|()| fs::rename(&tmp_path, path));
        if let Err(e) = result {
            let _ = fs::remove_file(&tmp_path);
            return Err(e).with_context(|| format!("Failed to write file: {:?}", path));
        }

        #[cfg(windows)]
        tracing::warn!(
            "File {:?} was written but Windows ACLs were not set — \
             protect it with folder-level permissions manually.",
            path
        );

        Ok(())
    }
}

#[async_trait::async_trait]
impl SecretBackend for FileBackend {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        debug!("Reading secret from file: {}", path);

        let file_path = self.path_to_file(path)?;

        if !file_path.exists() {
            anyhow::bail!("Secret file not found: {:?}", file_path);
        }

        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read secret file: {:?}", file_path))?;

        let mut data = HashMap::new();
        for line in content.lines() {
            if let Some((key, value)) = Self::parse_line(line) {
                data.insert(key, value);
            }
        }

        // Read metadata if it exists
        let metadata = self.read_metadata(path).await.ok();

        Ok(SecretData { data, metadata })
    }

    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()> {
        info!("Writing secret to file: {}", path);

        let file_path = self.path_to_file(path)?;
        self.ensure_parent_dir(&file_path)?;

        let mut content = String::new();
        content.push_str("# Secret file - Auto-generated by asr\n");
        content.push_str("# Format: key:value (one per line)\n\n");

        for (key, value) in &data {
            content.push_str(&Self::format_line(key, value));
        }

        Self::write_restricted(&file_path, &content)?;

        debug!("Successfully wrote secret to: {:?}", file_path);
        Ok(())
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()> {
        debug!("Updating metadata for: {}", path);

        let meta_path = self.metadata_path(path)?;
        self.ensure_parent_dir(&meta_path)?;

        let mut content = String::new();
        content.push_str("# Metadata file - Auto-generated by asr\n");
        content.push_str("# Format: key:value (one per line)\n\n");

        for (key, value) in &metadata {
            content.push_str(&Self::format_line(key, value));
        }

        Self::write_restricted(&meta_path, &content)?;

        Ok(())
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        let mut meta_path = self.metadata_path(path)?;
        if !meta_path.exists() {
            meta_path = self.legacy_metadata_path(path)?;
        }

        if !meta_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&meta_path)
            .with_context(|| format!("Failed to read metadata file: {:?}", meta_path))?;

        let mut metadata = HashMap::new();
        for line in content.lines() {
            if let Some((key, value)) = Self::parse_line(line) {
                metadata.insert(key, value);
            }
        }

        Ok(metadata)
    }

    async fn list_secrets(&self, path: &str) -> Result<Vec<String>> {
        debug!("Listing secrets at path: {}", path);

        let dir_path = if path.is_empty() {
            self.base_dir.clone()
        } else {
            self.path_to_file(path)?
        };

        if !dir_path.exists() {
            return Ok(Vec::new());
        }

        let mut secrets = Vec::new();

        // If it's a directory, list files in it
        if dir_path.is_dir() {
            let entries = fs::read_dir(&dir_path)
                .with_context(|| format!("Failed to read directory: {:?}", dir_path))?;

            for entry in entries {
                let entry = entry.context("Failed to read directory entry")?;
                let file_path = entry.path();

                // Skip metadata files and hidden files
                if file_path.extension().map(|e| e == "meta").unwrap_or(false) {
                    continue;
                }

                if file_path.is_file() {
                    // Strip dir_path (not base_dir) so callers receive names relative to the
                    // queried prefix — matches Vault's LIST behaviour and prevents scan_for_rotation
                    // from double-prefixing paths when called with a non-empty root.
                    if let Ok(relative) = file_path.strip_prefix(&dir_path) {
                        let secret_path = relative
                            .to_string_lossy()
                            .replace(std::path::MAIN_SEPARATOR, "/");
                        secrets.push(secret_path);
                    }
                } else if file_path.is_dir() {
                    // Recursively list secrets in subdirectories
                    let sub_name = file_path.file_name().unwrap().to_string_lossy().to_string();
                    let sub_path = if path.is_empty() {
                        sub_name.clone()
                    } else {
                        format!("{}/{}", path, sub_name)
                    };

                    let sub_secrets = self.list_secrets(&sub_path).await?;
                    for s in sub_secrets {
                        secrets.push(format!("{}/{}", sub_name, s));
                    }
                }
            }
        } else {
            // If it's a file, return just that file
            if let Ok(relative) = dir_path.strip_prefix(&self.base_dir) {
                secrets.push(relative.to_string_lossy().to_string());
            }
        }

        Ok(secrets)
    }

    fn backend_type(&self) -> &'static str {
        "file"
    }
}
