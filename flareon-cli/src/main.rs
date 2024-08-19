mod migration_generator;
mod utils;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

use crate::migration_generator::make_migrations;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    MakeMigrations { path: Option<PathBuf> },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    match cli.command {
        Commands::MakeMigrations { path } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            make_migrations(&path).with_context(|| "unable to create migrations")?;
        }
    }

    Ok(())
}
