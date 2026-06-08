use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use keyzen_core::Config;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "keyzen")]
#[command(version)]
#[command(about = "A fast, predictable, layer-based keyboard remapper for Windows.")]
struct Cli {
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate,
    Dump,
    Tray,
}

fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Validate) => {
            load_config(&config_path(cli.config))?;
            println!("configuration is valid");
            Ok(())
        }
        Some(Command::Dump) => {
            let config = load_config(&config_path(cli.config))?;
            print!("{}", config.to_yaml_string()?);
            Ok(())
        }
        Some(Command::Tray) => launch_tray(),
        None => match cli.config {
            Some(path) => run(path),
            None => launch_tray(),
        },
    }
}

fn run(path: PathBuf) -> Result<()> {
    let _instance = keyzen_win::single_instance::SingleInstance::acquire()
        .context("KeyZen is already running")?;
    let config = load_config(&path)?;
    let (stop_tx, stop_rx) = mpsc::channel();

    ctrlc::set_handler(move || {
        let _ = stop_tx.send(());
    })
    .context("failed to install Ctrl+C handler")?;

    info!("starting KeyZen with {}", path.display());
    keyzen_win::run_until_stop(config, stop_rx).context("KeyZen runtime failed")
}

fn launch_tray() -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to find the KeyZen executable")?;
    let tray_exe = current_exe.with_file_name(if cfg!(windows) {
        "keyzen-tray.exe"
    } else {
        "keyzen-tray"
    });
    if !tray_exe.exists() {
        anyhow::bail!(
            "keyzen-tray executable was not found at {}",
            tray_exe.display()
        );
    }

    let mut command = ProcessCommand::new(&tray_exe);
    configure_background_process(&mut command);
    command
        .spawn()
        .with_context(|| format!("failed to launch {}", tray_exe.display()))?;
    Ok(())
}

#[cfg(windows)]
fn configure_background_process(command: &mut ProcessCommand) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(windows))]
fn configure_background_process(_command: &mut ProcessCommand) {}

fn config_path(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(|| PathBuf::from("keyzen.yaml"))
}

fn load_config(path: &PathBuf) -> Result<Config> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    Config::from_yaml_str(&contents).with_context(|| format!("invalid config {}", path.display()))
}

fn init_logging() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("keyzen=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
