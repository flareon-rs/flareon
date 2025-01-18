//! An example of a custom task using clap's derive API that can be run from the
//! command line.
//!
//! Can be tested with:
//! ```bash
//! cargo run --bin example-custom-task -- frobnicate test
//! ```

use async_trait::async_trait;
use clap::{CommandFactory, FromArgMatches, Parser};
use cot::cli::clap::{ArgMatches, Command};
use cot::cli::CliTask;
use cot::{CotApp, CotProject};

#[derive(Parser)]
#[command(name = "frobnicate", about = "Frobnicate something")]
struct FrobnicateCommand {
    /// What should we frobnicate
    what: String,
}

struct Frobnicate;

#[async_trait]
impl CliTask for Frobnicate {
    fn subcommand(&self) -> Command {
        FrobnicateCommand::command()
    }

    async fn execute(&mut self, matches: &ArgMatches, _project: CotProject) -> cot::Result<()> {
        let command = FrobnicateCommand::from_arg_matches(matches).expect("invalid arguments");

        println!("Frobnicating {}...", command.what);

        Ok(())
    }
}

struct HelloApp;

impl CotApp for HelloApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }
}

#[cot::main]
async fn main() -> cot::Result<CotProject> {
    let cot_project = CotProject::builder()
        .with_cli(cot::cli::metadata!())
        .add_task(Frobnicate)
        .register_app_with_views(HelloApp, "")
        .build()
        .await?;

    Ok(cot_project)
}
