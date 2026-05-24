use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub start_at_login: bool,
    #[serde(default = "default_keymap_path")]
    pub keymap_path: PathBuf,
}

impl AppConfig {
    pub fn default_path() -> PathBuf {
        app_dir().join("config.toml")
    }

    pub fn ensure_default_file() -> Result<PathBuf> {
        let path = Self::default_path();
        Self::load_or_create(&path)?;
        Ok(path)
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        ensure_config_dir(path)?;
        if !path.exists() {
            let config = Self::default();
            config.save(path)?;
            return Ok(config);
        }

        let input = fs::read_to_string(path)
            .with_context(|| format!("failed to read app config {}", path.display()))?;
        toml::from_str(&input).context("failed to parse KeyZen app config")
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        ensure_config_dir(path)?;
        let output =
            toml::to_string_pretty(self).context("failed to serialize KeyZen app config")?;
        fs::write(path, output)
            .with_context(|| format!("failed to write app config {}", path.display()))?;
        Ok(())
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            start_at_login: false,
            keymap_path: default_keymap_path(),
        }
    }
}

fn default_keymap_path() -> PathBuf {
    app_dir().join("keyzen.toml")
}

fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("KeyZen")
}

fn ensure_config_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create KeyZen config directory")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn load_or_create_writes_default_config_when_missing() {
        let path = unique_temp_path().join("KeyZen").join("config.toml");
        assert!(!path.exists());

        let config = AppConfig::load_or_create(&path).unwrap();

        assert!(path.exists());
        assert!(!config.start_at_login);
        assert_eq!(config.keymap_path, default_keymap_path());

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("start_at_login = false"));
        assert!(written.contains("keymap_path"));
    }

    #[test]
    fn load_or_create_reads_existing_config() {
        let path = unique_temp_path().join("KeyZen").join("config.toml");
        let expected_keymap = path.with_file_name("custom.toml");
        let config = AppConfig {
            start_at_login: true,
            keymap_path: expected_keymap.clone(),
        };
        config.save(&path).unwrap();

        let loaded = AppConfig::load_or_create(&path).unwrap();

        assert!(loaded.start_at_login);
        assert_eq!(loaded.keymap_path, expected_keymap);
    }

    fn unique_temp_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("keyzen-app-config-test-{nanos}"))
    }
}
