extern crate core;

mod migration_generator;
mod utils;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use tracing_subscriber::util::SubscriberInitExt;

use crate::migration_generator::{make_migrations, MigrationGeneratorOptions};

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
    MakeMigrations {
        /// Path to the crate directory to generate migrations for (default:
        /// current directory)
        path: Option<PathBuf>,
        /// Directory to write the migrations to (default: migrations/ directory
        /// in the crate's src/ directory)
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(cli.verbose.tracing_level_filter().into()),
        )
        .finish()
        .init();

    match cli.command {
        Commands::MakeMigrations { path, output_dir } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            let options = MigrationGeneratorOptions { output_dir };
            make_migrations(&path, options).with_context(|| "unable to create migrations")?;
        }
    }

    Ok(())
}
