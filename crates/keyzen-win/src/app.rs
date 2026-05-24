use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result};
use keyzen_core::{Engine, RuntimeConfig};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM},
        UI::Controls::Dialogs::{
            GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR,
            OFN_PATHMUSTEXIST, OPENFILENAMEW,
        },
    },
    core::{PCWSTR, PWSTR, w},
};

use crate::{
    app_config::AppConfig, defaults::DEFAULT_KEYMAP, hook::KeyboardHook, log, startup, tray,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    TogglePause,
    ReloadConfig,
    OpenConfigFolder,
    SelectKeymapFile,
    ToggleStartAtLogin,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppStatus {
    Running,
    Paused,
    ConfigError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppState {
    pub status: AppStatus,
    pub start_at_login: bool,
}

pub struct KeyZenApp {
    app_config_path: PathBuf,
    app_config: AppConfig,
    engine: Arc<Mutex<Engine>>,
    paused: Arc<AtomicBool>,
    _hook: KeyboardHook,
    status: AppStatus,
}

impl KeyZenApp {
    pub fn new(app_config_path: PathBuf) -> Result<Self> {
        let app_config = AppConfig::load_or_create(&app_config_path)?;
        ensure_default_keymap(&app_config.keymap_path)?;
        if let Err(error) = startup::set_enabled(app_config.start_at_login) {
            let message = format!("KeyZen startup registration sync failed: {error:#}");
            eprintln!("{message}");
            log::error(message);
        }
        let config = load_keymap(&app_config.keymap_path)?;
        let engine = Arc::new(Mutex::new(Engine::new(config)));
        let paused = Arc::new(AtomicBool::new(false));
        let hook = KeyboardHook::install(engine.clone(), paused.clone())?;
        log::info("KeyZen keyboard hook installed");
        Ok(Self {
            app_config_path,
            app_config,
            engine,
            paused,
            _hook: hook,
            status: AppStatus::Running,
        })
    }

    pub fn run(mut self) -> Result<()> {
        let initial_state = self.state();
        tray::run_message_loop(move |command| self.handle_command(command), initial_state)
    }

    fn handle_command(&mut self, command: AppCommand) -> Result<AppState> {
        match command {
            AppCommand::TogglePause => {
                let new_paused = !self.paused.load(Ordering::Relaxed);
                self.paused.store(new_paused, Ordering::Relaxed);
                self.status = if new_paused {
                    AppStatus::Paused
                } else {
                    AppStatus::Running
                };
            }
            AppCommand::ReloadConfig => match load_keymap(&self.app_config.keymap_path) {
                Ok(config) => {
                    self.engine
                        .lock()
                        .expect("engine mutex poisoned")
                        .reload(config);
                    self.status = if self.paused.load(Ordering::Relaxed) {
                        AppStatus::Paused
                    } else {
                        AppStatus::Running
                    };
                }
                Err(error) => {
                    let message = format!("KeyZen config reload failed: {error:#}");
                    eprintln!("{message}");
                    log::error(message);
                    self.status = AppStatus::ConfigError;
                }
            },
            AppCommand::OpenConfigFolder => {
                if let Some(parent) = self.app_config_path.parent() {
                    open_folder(parent)?;
                }
            }
            AppCommand::SelectKeymapFile => {
                if let Some(path) = select_keymap_file()? {
                    match load_keymap(&path) {
                        Ok(config) => {
                            self.engine
                                .lock()
                                .expect("engine mutex poisoned")
                                .reload(config);
                            self.app_config.keymap_path = path;
                            self.app_config.save(&self.app_config_path)?;
                            self.status = if self.paused.load(Ordering::Relaxed) {
                                AppStatus::Paused
                            } else {
                                AppStatus::Running
                            };
                        }
                        Err(error) => {
                            let message = format!("KeyZen selected keymap failed: {error:#}");
                            eprintln!("{message}");
                            log::error(message);
                            self.status = AppStatus::ConfigError;
                        }
                    }
                }
            }
            AppCommand::ToggleStartAtLogin => {
                let enabled = !self.app_config.start_at_login;
                startup::set_enabled(enabled)?;
                self.app_config.start_at_login = enabled;
                self.app_config.save(&self.app_config_path)?;
            }
            AppCommand::Exit => tray::request_exit(),
        }

        Ok(self.state())
    }

    fn state(&self) -> AppState {
        AppState {
            status: self.status,
            start_at_login: self.app_config.start_at_login,
        }
    }
}

pub fn default_config_path() -> PathBuf {
    AppConfig::default_path()
}

pub fn ensure_default_config_path() -> Result<PathBuf> {
    AppConfig::ensure_default_file()
}

fn ensure_default_keymap(path: &PathBuf) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create KeyZen config directory")?;
    }
    fs::write(path, DEFAULT_KEYMAP).context("failed to write default KeyZen keymap")?;
    Ok(())
}

fn load_keymap(path: &PathBuf) -> Result<RuntimeConfig> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("failed to read keymap {}", path.display()))?;
    RuntimeConfig::parse(&input).context("failed to parse KeyZen config")
}

fn open_folder(path: &std::path::Path) -> Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .context("failed to open config folder")?;
    Ok(())
}

fn select_keymap_file() -> Result<Option<PathBuf>> {
    let mut file_buffer = [0u16; 1024];
    let filter = wide_double_null("TOML files (*.toml)\0*.toml\0All files (*.*)\0*.*\0");
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND::default(),
        hInstance: HINSTANCE::default(),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
        nMaxFile: file_buffer.len() as u32,
        lpstrTitle: w!("Select KeyZen keymap file"),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR,
        lCustData: LPARAM::default(),
        ..Default::default()
    };

    if unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        let len = file_buffer
            .iter()
            .position(|code| *code == 0)
            .unwrap_or(file_buffer.len());
        let path = String::from_utf16_lossy(&file_buffer[..len]);
        Ok(Some(PathBuf::from(path)))
    } else {
        Ok(None)
    }
}

fn wide_double_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn ensure_default_keymap_writes_builtin_keymap_when_missing() {
        let path = unique_temp_path().join("keyzen.toml");
        assert!(!path.exists());

        ensure_default_keymap(&path).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, DEFAULT_KEYMAP);
        RuntimeConfig::parse(&written).unwrap();
    }

    #[test]
    fn ensure_default_keymap_preserves_existing_file() {
        let path = unique_temp_path().join("keyzen.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let custom = r#"[settings]
startup_layer = "base"

[source]
keys = ["A"]

[layers.base]
A = "B"
"#;
        fs::write(&path, custom).unwrap();

        ensure_default_keymap(&path).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), custom);
    }

    fn unique_temp_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("keyzen-keymap-test-{nanos}"))
    }
}
