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

use crate::{hook::KeyboardHook, startup, tray};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    TogglePause,
    ReloadConfig,
    OpenConfigFolder,
    ToggleStartAtLogin,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppStatus {
    Running,
    Paused,
    ConfigError,
}

pub struct KeyZenApp {
    config_path: PathBuf,
    engine: Arc<Mutex<Engine>>,
    paused: Arc<AtomicBool>,
    _hook: KeyboardHook,
    status: AppStatus,
}

impl KeyZenApp {
    pub fn new(config_path: PathBuf) -> Result<Self> {
        ensure_default_config(&config_path)?;
        let config = load_config(&config_path)?;
        let engine = Arc::new(Mutex::new(Engine::new(config)));
        let paused = Arc::new(AtomicBool::new(false));
        let hook = KeyboardHook::install(engine.clone(), paused.clone())?;
        Ok(Self {
            config_path,
            engine,
            paused,
            _hook: hook,
            status: AppStatus::Running,
        })
    }

    pub fn run(mut self) -> Result<()> {
        let initial_status = self.status;
        tray::run_message_loop(move |command| self.handle_command(command), initial_status)
    }

    fn handle_command(&mut self, command: AppCommand) -> Result<AppStatus> {
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
            AppCommand::ReloadConfig => match load_config(&self.config_path) {
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
                    eprintln!("KeyZen config reload failed: {error:#}");
                    self.status = AppStatus::ConfigError;
                }
            },
            AppCommand::OpenConfigFolder => {
                if let Some(parent) = self.config_path.parent() {
                    open_folder(parent)?;
                }
            }
            AppCommand::ToggleStartAtLogin => {
                let enabled = startup::is_enabled().unwrap_or(false);
                startup::set_enabled(!enabled)?;
            }
            AppCommand::Exit => tray::request_exit(),
        }

        Ok(self.status)
    }
}

pub fn default_config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("KeyZen")
        .join("keyzen.toml")
}

fn ensure_default_config(path: &PathBuf) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create KeyZen config directory")?;
    }
    fs::write(path, include_str!("../../../examples/keyzen.toml"))
        .context("failed to write default KeyZen config")?;
    Ok(())
}

fn load_config(path: &PathBuf) -> Result<RuntimeConfig> {
    let input =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    RuntimeConfig::parse(&input).context("failed to parse KeyZen config")
}

fn open_folder(path: &std::path::Path) -> Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .context("failed to open config folder")?;
    Ok(())
}
