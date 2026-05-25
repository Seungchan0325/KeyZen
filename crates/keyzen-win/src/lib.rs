#[cfg(windows)]
pub mod app;
#[cfg(windows)]
mod app_config;
#[cfg(windows)]
mod defaults;
#[cfg(windows)]
pub mod dialog;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
mod keycode;
#[cfg(windows)]
pub mod log;
#[cfg(windows)]
mod output;
#[cfg(windows)]
mod startup;
#[cfg(windows)]
mod tray;

#[cfg(windows)]
pub use app::{AppCommand, AppState, AppStatus, KeyZenApp};

#[cfg(not(windows))]
compile_error!("keyzen-win only supports Windows targets");
