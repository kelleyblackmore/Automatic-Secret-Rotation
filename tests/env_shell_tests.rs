use anyhow::Result;
use secret_rotator::env_updater::EnvUpdater;
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
