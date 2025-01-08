mod migration_generator;
mod utils;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

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
        /// Name of the app to use in the migration (default: crate name)
        #[arg(long)]
        app_name: Option<String>,
        /// Directory to write the migrations to (default: migrations/ directory
        /// in the crate's src/ directory)
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    match cli.command {
        Commands::MakeMigrations {
            path,
            app_name,
            output_dir,
        } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            let options = MigrationGeneratorOptions {
                app_name,
                output_dir,
            };
            make_migrations(&path, options).with_context(|| "unable to create migrations")?;
        }
    }

    Ok(())
}
