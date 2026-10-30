use anyhow::{Context, Result};
use cap_std::fs::Dir;
use std::collections::HashMap;
use std::fs;
use std::io;
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
///
/// Every file operation goes through `dir`, a capability handle on the base
/// directory: paths are resolved relative to it, and anything that would
/// lead outside it (`..`, an absolute path, or a symlink, even one swapped
/// in mid-operation) fails instead of touching the outside file. Symlinks
/// that stay inside the directory keep working.
pub struct FileBackend {
    base_dir: PathBuf,
    dir: Dir,
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

        let dir = Dir::open_ambient_dir(&base_dir, cap_std::ambient_authority())
            .with_context(|| format!("Failed to open base directory: {:?}", base_dir))?;

        Ok(Self { base_dir, dir })
    }

    /// Validate a secret path and return it relative to the base directory.
    /// Example: "myapp/database" -> myapp/database
    ///
    /// Absolute paths, `..` and drive prefixes are refused up front for a
    /// clear error; `dir` would refuse them anyway, along with symlinks that
    /// point outside.
    fn relative_path(&self, path: &str) -> Result<PathBuf> {
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
        Ok(relative.to_path_buf())
    }

    /// Get metadata file path for a secret: the secret's file name plus
    /// `.meta`. (It used to replace the extension, so `db.v1` and `db.v2`
    /// shared `db.meta` and overwrote each other's rotation schedule.)
    fn metadata_path(&self, path: &str) -> Result<PathBuf> {
        let mut meta = self.relative_path(path)?.into_os_string();
        meta.push(".meta");
        Ok(PathBuf::from(meta))
    }

    /// Where metadata lived before 0.7.1, read as a fallback so existing
    /// rotation schedules for dotted names keep working until next written.
    fn legacy_metadata_path(&self, path: &str) -> Result<PathBuf> {
        Ok(self.relative_path(path)?.with_extension("meta"))
    }

    /// Read a file inside the base directory; `None` if it doesn't exist.
    fn read_file(&self, rel: &Path) -> Result<Option<String>> {
        match self.dir.read_to_string(rel) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e)
                .with_context(|| format!("Failed to read {:?} inside {:?}", rel, self.base_dir)),
        }
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

    /// Ensure parent directories exist for a path, owner-only on Unix
    fn ensure_parent_dir(&self, rel: &Path) -> Result<()> {
        let Some(parent) = rel.parent().filter(|p| !p.as_os_str().is_empty()) else {
            return Ok(());
        };
        let mut builder = cap_std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        cap_std::fs::DirBuilderExt::mode(&mut builder, 0o700);
        self.dir
            .create_dir_with(parent, &builder)
            .with_context(|| format!("Failed to create parent directory: {:?}", parent))?;
        // A directory that already existed keeps its mode unless tightened here.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            self.dir
                .set_permissions(
                    parent,
                    cap_std::fs::Permissions::from_std(fs::Permissions::from_mode(0o700)),
                )
                .with_context(|| format!("Failed to set permissions on {:?}", parent))?;
        }
        Ok(())
    }

    /// Write content to a file readable only by its owner (0o600 on Unix).
    ///
    /// The content goes to a temporary file in the same directory that is
    /// created with mode 0o600, then renamed over the target. The secret is
    /// therefore never in a file other users can read, not even briefly, and
    /// a crash mid-write leaves the old secret intact rather than half a file.
    fn write_restricted(&self, rel: &Path, content: &str) -> Result<()> {
        use std::io::Write;

        let file_name = rel
            .file_name()
            .with_context(|| format!("Invalid secret file path: {:?}", rel))?
            .to_string_lossy();
        let tmp = rel.with_file_name(format!(
            ".{}.tmp-{}-{}",
            file_name,
            std::process::id(),
            rand::random::<u32>()
        ));

        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        cap_std::fs::OpenOptionsExt::mode(&mut options, 0o600);

        let result = self
            .dir
            .open_with(&tmp, &options)
            .and_then(|mut f| {
                f.write_all(content.as_bytes())?;
                f.sync_all()
            })
            .and_then(|()| self.dir.rename(&tmp, &self.dir, rel));
        if let Err(e) = result {
            let _ = self.dir.remove_file(&tmp);
            return Err(e)
                .with_context(|| format!("Failed to write {:?} inside {:?}", rel, self.base_dir));
        }

        #[cfg(windows)]
        tracing::warn!(
            "File {:?} was written but Windows ACLs were not set — \
             protect it with folder-level permissions manually.",
            rel
        );

        Ok(())
    }
}

#[async_trait::async_trait]
impl SecretBackend for FileBackend {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        debug!("Reading secret from file: {}", path);

        let rel = self.relative_path(path)?;
        let content = self
            .read_file(&rel)?
            .with_context(|| format!("Secret file not found: {:?}", self.base_dir.join(&rel)))?;

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

        let rel = self.relative_path(path)?;
        self.ensure_parent_dir(&rel)?;

        let mut content = String::new();
        content.push_str("# Secret file - Auto-generated by asr\n");
        content.push_str("# Format: key:value (one per line)\n\n");

        for (key, value) in &data {
            content.push_str(&Self::format_line(key, value));
        }

        self.write_restricted(&rel, &content)?;

        debug!("Successfully wrote secret to: {:?}", rel);
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

        self.write_restricted(&meta_path, &content)?;

        Ok(())
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        let content = match self.read_file(&self.metadata_path(path)?)? {
            Some(content) => content,
            None => match self.read_file(&self.legacy_metadata_path(path)?)? {
                Some(content) => content,
                None => return Ok(HashMap::new()),
            },
        };

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

        let rel = if path.is_empty() {
            PathBuf::from(".")
        } else {
            self.relative_path(path)?
        };

        let meta = match self.dir.metadata(&rel) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("Failed to stat {:?} inside {:?}", rel, self.base_dir)
                })
            }
        };

        // If it's a file, return just that file
        if !meta.is_dir() {
            return Ok(vec![rel
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")]);
        }

        let entries = self
            .dir
            .read_dir(&rel)
            .with_context(|| format!("Failed to read directory: {:?}", rel))?;

        let mut secrets = Vec::new();
        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip metadata files and in-progress temporary files
            if name.ends_with(".meta") || (name.starts_with('.') && name.contains(".tmp-")) {
                continue;
            }

            // Follows symlinks only while they stay inside the base directory;
            // one pointing outside is skipped rather than listed.
            let child = rel.join(&name);
            let Ok(child_meta) = self.dir.metadata(&child) else {
                debug!(
                    "Skipping {:?}: not reachable inside {:?}",
                    child, self.base_dir
                );
                continue;
            };

            if child_meta.is_file() {
                // Names are relative to the queried prefix — matches Vault's LIST
                // behaviour and prevents scan_for_rotation from double-prefixing
                // paths when called with a non-empty root.
                secrets.push(name);
            } else if child_meta.is_dir() {
                // Recursively list secrets in subdirectories
                let sub_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", path, name)
                };

                let sub_secrets = self.list_secrets(&sub_path).await?;
                for s in sub_secrets {
                    secrets.push(format!("{}/{}", name, s));
                }
            }
        }

        Ok(secrets)
    }

    fn backend_type(&self) -> &'static str {
        "file"
    }
}
