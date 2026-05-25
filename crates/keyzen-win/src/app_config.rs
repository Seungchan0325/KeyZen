use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub start_at_login: bool,
    #[serde(default = "default_keymap_path")]
    pub keymap_path: PathBuf,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default)]
    pub level: LogLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default = "default_log_max_bytes")]
    pub max_bytes: u64,
    #[serde(default = "default_log_max_files")]
    pub max_files: u8,
}

impl LoggingConfig {
    pub fn validate(&self) -> Result<()> {
        if self.max_bytes == 0 {
            bail!("logging.max_bytes must be greater than 0");
        }
        if self.max_files == 0 {
            bail!("logging.max_files must be greater than 0");
        }
        Ok(())
    }

    pub fn resolved_path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(default_log_path)
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            path: None,
            max_bytes: default_log_max_bytes(),
            max_files: default_log_max_files(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_level_filter(self) -> log::LevelFilter {
        match self {
            Self::Error => log::LevelFilter::Error,
            Self::Warn => log::LevelFilter::Warn,
            Self::Info => log::LevelFilter::Info,
            Self::Debug => log::LevelFilter::Debug,
            Self::Trace => log::LevelFilter::Trace,
        }
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
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
        let config: Self = toml::from_str(&input).context("failed to parse KeyZen app config")?;
        config.logging.validate()?;
        Ok(config)
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
            logging: LoggingConfig::default(),
        }
    }
}

fn default_keymap_path() -> PathBuf {
    app_dir().join("keyzen.toml")
}

pub fn default_log_path() -> PathBuf {
    app_dir().join("keyzen.log")
}

fn default_log_max_bytes() -> u64 {
    1024 * 1024
}

fn default_log_max_files() -> u8 {
    3
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
        assert_eq!(config.logging.level, LogLevel::Info);
        assert_eq!(
            config.logging.resolved_path().file_name().unwrap(),
            "keyzen.log"
        );

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("start_at_login = false"));
        assert!(written.contains("keymap_path"));
        assert!(written.contains("[logging]"));
    }

    #[test]
    fn load_or_create_reads_existing_config() {
        let path = unique_temp_path().join("KeyZen").join("config.toml");
        let expected_keymap = path.with_file_name("custom.toml");
        let config = AppConfig {
            start_at_login: true,
            keymap_path: expected_keymap.clone(),
            logging: LoggingConfig::default(),
        };
        config.save(&path).unwrap();

        let loaded = AppConfig::load_or_create(&path).unwrap();

        assert!(loaded.start_at_login);
        assert_eq!(loaded.keymap_path, expected_keymap);
    }

    #[test]
    fn parses_logging_levels() {
        let input = r#"
start_at_login = false
keymap_path = "C:\\KeyZen\\keyzen.toml"

[logging]
level = "debug"
max_bytes = 1024
max_files = 2
"#;

        let config: AppConfig = toml::from_str(input).unwrap();

        assert_eq!(config.logging.level, LogLevel::Debug);
    }

    #[test]
    fn rejects_invalid_logging_level() {
        let input = r#"
start_at_login = false
keymap_path = "C:\\KeyZen\\keyzen.toml"

[logging]
level = "verbose"
"#;

        assert!(toml::from_str::<AppConfig>(input).is_err());
    }

    #[test]
    fn rejects_zero_logging_limits() {
        let config = LoggingConfig {
            max_bytes: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = LoggingConfig {
            max_files: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    fn unique_temp_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("keyzen-app-config-test-{nanos}"))
    }
}
