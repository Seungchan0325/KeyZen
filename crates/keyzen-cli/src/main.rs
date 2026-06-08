use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use keyzen_core::Config;
use std::path::PathBuf;
use std::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "keyzen")]
#[command(about = "A fast, predictable, layer-based keyboard remapper for Windows.")]
struct Cli {
    #[arg(short, long, global = true, default_value = "keyzen.yaml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate,
    Dump,
}

fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Validate) => {
            load_config(&cli.config)?;
            println!("configuration is valid");
            Ok(())
        }
        Some(Command::Dump) => {
            let config = load_config(&cli.config)?;
            print!("{}", config.to_yaml_string()?);
            Ok(())
        }
        None => run(cli.config),
    }
}

fn run(path: PathBuf) -> Result<()> {
    let config = load_config(&path)?;
    let (stop_tx, stop_rx) = mpsc::channel();

    ctrlc::set_handler(move || {
        let _ = stop_tx.send(());
    })
    .context("failed to install Ctrl+C handler")?;

    info!("starting KeyZen with {}", path.display());
    keyzen_win::run_until_stop(config, stop_rx).context("KeyZen runtime failed")
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
