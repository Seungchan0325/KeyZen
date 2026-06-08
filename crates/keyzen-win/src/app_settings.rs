use keyzen_core::Config;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const DEFAULT_KEY_CONFIG: &str = "keyzen.yaml";
const SETTINGS_FILE: &str = "settings.yaml";
const EMPTY_KEY_CONFIG: &str = "layers:\n  base: {}\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub start_at_login: bool,
    pub key_config_path: PathBuf,
}

impl AppSettings {
    pub fn with_default_key_config(key_config_path: PathBuf) -> Self {
        Self {
            start_at_login: false,
            key_config_path,
        }
    }

    pub fn resolve_key_config_path(&self, settings_path: &Path) -> PathBuf {
        if self.key_config_path.is_absolute() {
            self.key_config_path.clone()
        } else {
            settings_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&self.key_config_path)
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::with_default_key_config(PathBuf::from(DEFAULT_KEY_CONFIG))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub dir: PathBuf,
    pub settings: PathBuf,
    pub default_key_config: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, AppSettingsError> {
        let appdata = std::env::var_os("APPDATA").ok_or(AppSettingsError::MissingAppData)?;
        Ok(Self::under(PathBuf::from(appdata).join("KeyZen")))
    }

    pub fn under(dir: PathBuf) -> Self {
        Self {
            settings: dir.join(SETTINGS_FILE),
            default_key_config: dir.join(DEFAULT_KEY_CONFIG),
            dir,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppSettingsError {
    #[error("APPDATA environment variable is not set")]
    MissingAppData,
    #[error("I/O error at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("failed to parse app settings {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_yaml::Error,
    },
    #[error("failed to serialize app settings: {0}")]
    Serialize(#[from] serde_yaml::Error),
    #[error("invalid key config {path}: {source}")]
    KeyConfig {
        path: PathBuf,
        source: keyzen_core::ConfigError,
    },
}

pub fn load_or_bootstrap() -> Result<(AppPaths, AppSettings), AppSettingsError> {
    let paths = AppPaths::discover()?;
    let settings = load_or_bootstrap_at(&paths)?;
    Ok((paths, settings))
}

pub fn load_or_bootstrap_at(paths: &AppPaths) -> Result<AppSettings, AppSettingsError> {
    fs::create_dir_all(&paths.dir).map_err(|source| AppSettingsError::Io {
        path: paths.dir.clone(),
        source,
    })?;

    if !paths.default_key_config.exists() {
        write_atomic(&paths.default_key_config, EMPTY_KEY_CONFIG.as_bytes())?;
    }

    if !paths.settings.exists() {
        let settings = AppSettings::with_default_key_config(paths.default_key_config.clone());
        save_at(paths, &settings)?;
        return Ok(settings);
    }

    load_at(paths)
}

pub fn load_at(paths: &AppPaths) -> Result<AppSettings, AppSettingsError> {
    let text = fs::read_to_string(&paths.settings).map_err(|source| AppSettingsError::Io {
        path: paths.settings.clone(),
        source,
    })?;
    serde_yaml::from_str(&text).map_err(|source| AppSettingsError::Parse {
        path: paths.settings.clone(),
        source,
    })
}

pub fn save_at(paths: &AppPaths, settings: &AppSettings) -> Result<(), AppSettingsError> {
    fs::create_dir_all(&paths.dir).map_err(|source| AppSettingsError::Io {
        path: paths.dir.clone(),
        source,
    })?;
    let bytes = serde_yaml::to_string(settings)?.into_bytes();
    write_atomic(&paths.settings, &bytes)
}

pub fn load_key_config(
    settings_path: &Path,
    settings: &AppSettings,
) -> Result<Config, AppSettingsError> {
    let path = settings.resolve_key_config_path(settings_path);
    let text = fs::read_to_string(&path).map_err(|source| AppSettingsError::Io {
        path: path.clone(),
        source,
    })?;
    Config::from_yaml_str(&text).map_err(|source| AppSettingsError::KeyConfig { path, source })
}

pub fn validate_key_config(path: &Path) -> Result<Config, AppSettingsError> {
    let text = fs::read_to_string(path).map_err(|source| AppSettingsError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Config::from_yaml_str(&text).map_err(|source| AppSettingsError::KeyConfig {
        path: path.to_path_buf(),
        source,
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppSettingsError> {
    let tmp = path.with_extension(format!(
        "{}tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));

    fs::write(&tmp, bytes).map_err(|source| AppSettingsError::Io {
        path: tmp.clone(),
        source,
    })?;

    replace_file(&tmp, path).map_err(|source| AppSettingsError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(windows)]
fn replace_file(tmp: &Path, path: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let tmp_w = wide_path(tmp);
    let path_w = wide_path(path);
    let ok = unsafe {
        MoveFileExW(
            tmp_w.as_ptr(),
            path_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(not(windows))]
fn replace_file(tmp: &Path, path: &Path) -> io::Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::{
        AppPaths, AppSettings, load_key_config, load_or_bootstrap_at, save_at, validate_key_config,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("keyzen-{name}-{nonce}"))
    }

    #[test]
    fn bootstraps_default_settings_and_key_config() {
        let root = temp_dir("bootstrap");
        let paths = AppPaths::under(root);

        let settings = load_or_bootstrap_at(&paths).unwrap();

        assert!(!settings.start_at_login);
        assert_eq!(settings.key_config_path, paths.default_key_config);
        assert!(paths.settings.exists());
        assert!(paths.default_key_config.exists());
        validate_key_config(&paths.default_key_config).unwrap();
    }

    #[test]
    fn roundtrips_settings_yaml() {
        let root = temp_dir("roundtrip");
        let paths = AppPaths::under(root);
        let settings = AppSettings {
            start_at_login: true,
            key_config_path: PathBuf::from("custom.yaml"),
        };

        save_at(&paths, &settings).unwrap();
        let loaded = load_or_bootstrap_at(&paths).unwrap();

        assert_eq!(loaded, settings);
        assert!(!paths.settings.with_extension("yaml.tmp").exists());
    }

    #[test]
    fn resolves_relative_key_config_paths_against_settings_file() {
        let root = temp_dir("relative");
        let paths = AppPaths::under(root.clone());
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("custom.yaml"), "layers:\n  base: {}\n").unwrap();
        let settings = AppSettings {
            start_at_login: false,
            key_config_path: PathBuf::from("custom.yaml"),
        };

        let config = load_key_config(&paths.settings, &settings).unwrap();

        assert!(config.layers.contains_key("base"));
    }
}
