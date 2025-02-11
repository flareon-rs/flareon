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
use cot::cli::{Cli, CliMetadata, CliTask};
use cot::config::ProjectConfig;
use cot::project::WithConfig;
use cot::{Bootstrapper, Project};

#[derive(Parser)]
#[command(name = "frobnicate", about = "Frobnicate something")]
struct FrobnicateCommand {
    /// What should we frobnicate
    what: String,
}

struct Frobnicate;

#[async_trait(?Send)]
impl CliTask for Frobnicate {
    fn subcommand(&self) -> Command {
        FrobnicateCommand::command()
    }

    async fn execute(
        &mut self,
        matches: &ArgMatches,
        _bootstrapper: Bootstrapper<WithConfig>,
    ) -> cot::Result<()> {
        let command = FrobnicateCommand::from_arg_matches(matches).expect("invalid arguments");

        println!("Frobnicating {}...", command.what);

        Ok(())
    }
}

struct CustomTaskProject;

impl Project for CustomTaskProject {
    fn cli_metadata(&self) -> CliMetadata {
        cot::cli::metadata!()
    }

    fn config(&self, _config_name: &str) -> cot::Result<ProjectConfig> {
        Ok(ProjectConfig::dev_default())
    }

    fn register_tasks(&self, cli: &mut Cli) {
        cli.add_task(Frobnicate)
    }
}

#[cot::main]
fn main() -> impl Project {
    CustomTaskProject
}
