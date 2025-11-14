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
    #[cfg_attr(not(test), allow(dead_code))] // Used in tests
    pub fn with_home_dir(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }

    /// Update or add an environment variable in shell config files
    pub fn update_env_var(&self, var_name: &str, new_value: &str) -> Result<()> {
        info!("Updating environment variable: {}", var_name);

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
                new_content.push_str(&format!("export {}=\"{}\"\n", var_name, new_value));
                found = true;
            } else {
                new_content.push_str(line);
                new_content.push('\n');
            }
        }

        if found {
            fs::write(path, new_content)
                .with_context(|| format!("Failed to write to {}", path.display()))?;
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
        content.push_str(&format!(
            "\n# Auto-updated by secret rotator\nexport {}=\"{}\"\n",
            var_name, new_value
        ));

        fs::write(path, content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;

        Ok(())
    }

    /// Remove an environment variable from shell config files
    ///
    /// Note: This is primarily used for testing. For production use, consider
    /// manually editing shell config files or using standard shell utilities.
    #[cfg(test)]
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
    #[cfg(test)]
    fn remove_from_file(&self, path: &Path, var_name: &str) -> Result<()> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let export_pattern = format!("export {}=", var_name);
        let mut new_content = String::new();
        let mut skip_next_line = false;

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip the auto-update comment and the next line (the export)
            if trimmed == "# Auto-updated by secret rotator" {
                skip_next_line = true;
                continue;
            }

            // Skip commented out lines
            if trimmed.starts_with('#') {
                new_content.push_str(line);
                new_content.push('\n');
                continue;
            }

            // Check if this line exports our variable
            if trimmed.starts_with(&export_pattern)
                || trimmed.starts_with(&format!("{}=", var_name))
            {
                // Skip this line (and reset skip flag if it was set)
                skip_next_line = false;
                continue;
            }

            // If we were supposed to skip this line, skip it
            if skip_next_line {
                skip_next_line = false;
                continue;
            }

            new_content.push_str(line);
            new_content.push('\n');
        }

        fs::write(path, new_content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_update_new_variable() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "# existing config\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        updater.update_env_var("MY_SECRET", "new_value")?;

        let content = fs::read_to_string(&bashrc)?;
        assert!(content.contains("export MY_SECRET=\"new_value\""));

        Ok(())
    }

    #[test]
    fn test_update_existing_variable() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "export MY_SECRET=\"old_value\"\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        updater.update_env_var("MY_SECRET", "new_value")?;

        let content = fs::read_to_string(&bashrc)?;
        assert!(content.contains("export MY_SECRET=\"new_value\""));
        assert!(!content.contains("old_value"));

        Ok(())
    }

    #[test]
    fn test_remove_variable() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "export MY_SECRET=\"value\"\n# other config\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        updater.remove_env_var("MY_SECRET")?;

        let content = fs::read_to_string(&bashrc)?;
        assert!(!content.contains("MY_SECRET"));
        assert!(content.contains("# other config"));

        Ok(())
    }
}
