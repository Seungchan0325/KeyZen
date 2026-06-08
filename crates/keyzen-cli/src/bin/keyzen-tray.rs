#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    keyzen_win::tray::run_tray_app()?;
    Ok(())
}
