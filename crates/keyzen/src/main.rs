#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    let app = keyzen_win::KeyZenApp::new(keyzen_win::app::default_config_path())?;
    app.run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KeyZen MVP currently supports Windows only.");
}
