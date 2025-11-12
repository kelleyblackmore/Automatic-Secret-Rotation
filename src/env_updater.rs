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
    pub fn with_home_dir(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }

    /// Escape a value for safe use in shell double quotes
    /// Escapes backslashes, double quotes, dollar signs, backticks, and newlines
    fn escape_shell_value(value: &str) -> String {
        value
            .replace('\\', r"\\")  // Escape backslashes first
            .replace('"', r#"\""#)  // Escape double quotes
            .replace('$', r"\$")    // Escape dollar signs (prevent variable expansion)
            .replace('`', r"\`")    // Escape backticks (prevent command substitution)
            .replace('\n', r"\n")   // Escape newlines
    }

    /// Update or add an environment variable in shell config files
    pub fn update_env_var(&self, var_name: &str, new_value: &str) -> Result<()> {
        info!("Updating environment variable: {}", var_name);

        // Common shell config files
        let config_files = vec![
            ".bashrc",
            ".bash_profile",
            ".zshrc",
            ".profile",
        ];

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
        let escaped_value = Self::escape_shell_value(new_value);

        for line in content.lines() {
            let trimmed = line.trim();
            
            // Check if this line exports our variable
            if trimmed.starts_with(&export_pattern) || 
               trimmed.starts_with(&format!("{}=", var_name)) {
                // Replace the line with the new value
                new_content.push_str(&format!("export {}=\"{}\"\n", var_name, escaped_value));
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

        let escaped_value = Self::escape_shell_value(new_value);
        
        // Add a comment and the new export
        content.push_str(&format!(
            "\n# Auto-updated by secret rotator\nexport {}=\"{}\"\n",
            var_name, escaped_value
        ));

        fs::write(path, content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;

        Ok(())
    }

    /// Remove an environment variable from shell config files
    pub fn remove_env_var(&self, var_name: &str) -> Result<()> {
        info!("Removing environment variable: {}", var_name);

        let config_files = vec![
            ".bashrc",
            ".bash_profile",
            ".zshrc",
            ".profile",
        ];

        for config_file in config_files {
            let config_path = self.home_dir.join(config_file);
            
            if config_path.exists() {
                self.remove_from_file(&config_path, var_name)?;
            }
        }

        Ok(())
    }

    /// Remove environment variable from a specific file
    fn remove_from_file(&self, path: &Path, var_name: &str) -> Result<()> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let export_pattern = format!("export {}=", var_name);
        let mut new_content = String::new();
        let mut skip_next_comment = false;

        for line in content.lines() {
            let trimmed = line.trim();
            
            // Skip the auto-update comment if we're about to remove a variable
            if trimmed == "# Auto-updated by secret rotator" {
                skip_next_comment = true;
                continue;
            }

            // Check if this line exports our variable
            if trimmed.starts_with(&export_pattern) || 
               trimmed.starts_with(&format!("{}=", var_name)) {
                skip_next_comment = false;
                continue; // Skip this line
            }

            if skip_next_comment {
                skip_next_comment = false;
                new_content.push_str(line);
                new_content.push('\n');
            } else {
                new_content.push_str(line);
                new_content.push('\n');
            }
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

    #[test]
    fn test_escape_shell_value() {
        // Test escaping double quotes
        assert_eq!(
            EnvUpdater::escape_shell_value(r#"value"with"quotes"#),
            r#"value\"with\"quotes"#
        );

        // Test escaping backslashes
        assert_eq!(
            EnvUpdater::escape_shell_value(r"path\to\file"),
            r"path\\to\\file"
        );

        // Test escaping dollar signs (prevent variable expansion)
        assert_eq!(
            EnvUpdater::escape_shell_value("value$VAR$test"),
            r"value\$VAR\$test"
        );

        // Test escaping backticks (prevent command substitution)
        assert_eq!(
            EnvUpdater::escape_shell_value("value`cmd`test"),
            r"value\`cmd\`test"
        );

        // Test escaping newlines
        assert_eq!(
            EnvUpdater::escape_shell_value("line1\nline2"),
            r"line1\nline2"
        );

        // Test escaping multiple special characters at once
        assert_eq!(
            EnvUpdater::escape_shell_value(r#"test"$VAR`cmd`\path"#),
            r#"test\"\$VAR\`cmd\`\\path"#
        );
    }

    #[test]
    fn test_update_variable_with_special_characters() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "# existing config\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        
        // Test with value containing double quotes
        updater.update_env_var("MY_SECRET", r#"value"with"quotes"#)?;
        let content = fs::read_to_string(&bashrc)?;
        assert!(content.contains(r#"export MY_SECRET="value\"with\"quotes""#));
        
        Ok(())
    }

    #[test]
    fn test_update_variable_with_command_injection_attempt() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "export MY_SECRET=\"old_value\"\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        
        // Try to inject a command using backticks
        updater.update_env_var("MY_SECRET", "test`whoami`test")?;
        let content = fs::read_to_string(&bashrc)?;
        // Should be escaped, not executed
        assert!(content.contains("export MY_SECRET=\"test\\`whoami\\`test\""));
        
        // Try to inject using $()
        updater.update_env_var("MY_SECRET", "test$(whoami)test")?;
        let content = fs::read_to_string(&bashrc)?;
        // Dollar signs should be escaped
        assert!(content.contains("export MY_SECRET=\"test\\$(whoami)test\""));
        
        Ok(())
    }

    #[test]
    fn test_update_variable_with_backslashes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "# existing config\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        
        // Test with Windows-style path
        updater.update_env_var("MY_PATH", r"C:\Users\test\file.txt")?;
        let content = fs::read_to_string(&bashrc)?;
        assert!(content.contains(r#"export MY_PATH="C:\\Users\\test\\file.txt""#));
        
        Ok(())
    }
}
