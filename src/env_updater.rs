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

    /// Escape special shell characters in a value to prevent injection
    /// 
    /// This function escapes characters that have special meaning in shell double quotes:
    /// - Backslash (\) - escape character
    /// - Double quote (") - string delimiter
    /// - Dollar sign ($) - variable expansion
    /// - Backtick (`) - command substitution
    /// - Newline - line terminator
    fn escape_shell_value(value: &str) -> String {
        value
            .replace('\\', r"\\")  // Must be first to avoid double-escaping
            .replace('"', r#"\""#)
            .replace('$', r"\$")
            .replace('`', r"\`")
            .replace('\n', r"\n")
    }

    /// Create an EnvUpdater for a specific home directory
    pub fn with_home_dir(home_dir: PathBuf) -> Self {
        Self { home_dir }
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

        for line in content.lines() {
            let trimmed = line.trim();
            
            // Check if this line exports our variable
            if trimmed.starts_with(&export_pattern) || 
               trimmed.starts_with(&format!("{}=", var_name)) {
                // Replace the line with the new value (escaped to prevent injection)
                let escaped_value = Self::escape_shell_value(new_value);
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

        // Add a comment and the new export (escaped to prevent injection)
        let escaped_value = Self::escape_shell_value(new_value);
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
    fn test_escape_shell_value_backslash() -> Result<()> {
        let escaped = EnvUpdater::escape_shell_value(r"path\to\file");
        assert_eq!(escaped, r"path\\to\\file");
        Ok(())
    }

    #[test]
    fn test_escape_shell_value_double_quotes() -> Result<()> {
        let escaped = EnvUpdater::escape_shell_value(r#"value with "quotes""#);
        assert_eq!(escaped, r#"value with \"quotes\""#);
        Ok(())
    }

    #[test]
    fn test_escape_shell_value_dollar_sign() -> Result<()> {
        let escaped = EnvUpdater::escape_shell_value("value with $VAR");
        assert_eq!(escaped, r"value with \$VAR");
        Ok(())
    }

    #[test]
    fn test_escape_shell_value_backtick() -> Result<()> {
        let escaped = EnvUpdater::escape_shell_value("value with `cmd`");
        assert_eq!(escaped, r"value with \`cmd\`");
        Ok(())
    }

    #[test]
    fn test_escape_shell_value_newline() -> Result<()> {
        let escaped = EnvUpdater::escape_shell_value("line1\nline2");
        assert_eq!(escaped, r"line1\nline2");
        Ok(())
    }

    #[test]
    fn test_escape_shell_value_combined() -> Result<()> {
        let escaped = EnvUpdater::escape_shell_value(r#"complex\value"with$special`chars"#);
        assert_eq!(escaped, r#"complex\\value\"with\$special\`chars"#);
        Ok(())
    }

    #[test]
    fn test_update_variable_with_special_chars() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "# existing config\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        // Test value with various shell metacharacters
        updater.update_env_var("MY_SECRET", r#"pa$$word"with'quotes`and\backslashes"#)?;

        let content = fs::read_to_string(&bashrc)?;
        // The value should be properly escaped
        assert!(content.contains(r#"export MY_SECRET="pa\$\$word\"with'quotes\`and\\backslashes""#));
        
        Ok(())
    }

    #[test]
    fn test_update_existing_variable_with_injection_attempt() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bashrc = temp_dir.path().join(".bashrc");
        fs::write(&bashrc, "export MY_SECRET=\"old_value\"\n")?;

        let updater = EnvUpdater::with_home_dir(temp_dir.path().to_path_buf());
        // Attempt to inject a command
        updater.update_env_var("MY_SECRET", r#""; rm -rf / #"#)?;

        let content = fs::read_to_string(&bashrc)?;
        // The injection attempt should be escaped - the double quote should be escaped
        assert!(content.contains(r#"export MY_SECRET="\"; rm -rf / #""#));
        // Should not contain unescaped version that could execute
        assert!(!content.contains(r#"export MY_SECRET=""; rm -rf / #""#));
        
        Ok(())
    }
}
