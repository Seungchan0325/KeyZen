use anyhow::{Context, Result, bail};
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

    #[arg(
        long,
        global = true,
        help = "Print structured key input/output diagnostics in foreground mode"
    )]
    debug_events: bool,

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
    let cli = Cli::parse();
    validate_cli(&cli)?;
    init_logging(cli.debug_events);
    let debug_events = cli.debug_events;

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
            Some(path) => run(path, debug_events),
            None => launch_tray(),
        },
    }
}

fn validate_cli(cli: &Cli) -> Result<()> {
    if cli.debug_events && cli.command.is_some() {
        bail!("--debug-events can only be used with foreground remapping");
    }
    if cli.debug_events && cli.config.is_none() {
        bail!("--debug-events requires --config for foreground remapping");
    }
    Ok(())
}

fn run(path: PathBuf, debug_events: bool) -> Result<()> {
    let _instance = keyzen_win::single_instance::SingleInstance::acquire()
        .context("KeyZen is already running")?;
    let config = load_config(&path)?;
    let (stop_tx, stop_rx) = mpsc::channel();

    ctrlc::set_handler(move || {
        let _ = stop_tx.send(());
    })
    .context("failed to install Ctrl+C handler")?;

    info!("starting KeyZen with {}", path.display());
    keyzen_win::run_until_stop_with_debug_events(config, stop_rx, debug_events)
        .context("KeyZen runtime failed")
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

fn init_logging(debug_events: bool) {
    let mut filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("keyzen=info"));
    if debug_events {
        filter = filter.add_directive(
            format!("{}=debug", keyzen_win::DEBUG_EVENTS_TARGET)
                .parse()
                .expect("debug event log directive should be valid"),
        );
    }
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::{Cli, validate_cli};
    use clap::Parser;

    #[test]
    fn accepts_debug_events_for_foreground_remapping() {
        let cli = Cli::try_parse_from([
            "keyzen",
            "--config",
            "examples/keyzen.yaml",
            "--debug-events",
        ])
        .unwrap();

        assert!(cli.debug_events);
        assert!(validate_cli(&cli).is_ok());
    }

    #[test]
    fn rejects_debug_events_without_config() {
        let cli = Cli::try_parse_from(["keyzen", "--debug-events"]).unwrap();

        assert_eq!(
            validate_cli(&cli).unwrap_err().to_string(),
            "--debug-events requires --config for foreground remapping"
        );
    }

    #[test]
    fn rejects_debug_events_with_subcommands() {
        for subcommand in ["validate", "dump", "tray"] {
            let cli = Cli::try_parse_from([
                "keyzen",
                subcommand,
                "--config",
                "examples/keyzen.yaml",
                "--debug-events",
            ])
            .unwrap();

            assert_eq!(
                validate_cli(&cli).unwrap_err().to_string(),
                "--debug-events can only be used with foreground remapping"
            );
        }
    }
}
