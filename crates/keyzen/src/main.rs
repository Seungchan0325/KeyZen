#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    let app_config_path = keyzen_win::app::ensure_default_config_path()?;
    let app = keyzen_win::KeyZenApp::new(app_config_path)?;
    app.run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KeyZen MVP currently supports Windows only.");
}
