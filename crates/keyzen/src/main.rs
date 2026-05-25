#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    keyzen_win::log::init_default();
    if let Err(error) = run() {
        ::log::error!("KeyZen fatal error: {error:#}");
        keyzen_win::dialog::show_fatal_error(&error);
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run() -> anyhow::Result<()> {
    ::log::info!(
        "KeyZen starting; log_path={}",
        keyzen_win::log::path().display()
    );
    let app_config_path = keyzen_win::app::ensure_default_config_path()?;
    ::log::info!("KeyZen config path={}", app_config_path.display());
    let app = keyzen_win::KeyZenApp::new(app_config_path)?;
    app.run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KeyZen MVP currently supports Windows only.");
}
