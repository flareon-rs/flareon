use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
pub use clap;
use clap::{value_parser, Arg, ArgMatches, Command};
use cot::error::ErrorRepr;
use cot::CotProject;
use derive_more::Debug;

use crate::{Error, Result};

const COLLECT_STATIC_SUBCOMMAND: &str = "collect-static";
const LISTEN_PARAM: &str = "listen";
const COLLECT_STATIC_DIR_PARAM: &str = "dir";

#[derive(Debug)]
pub(crate) struct Cli {
    command: Command,
    #[debug("...")]
    tasks: HashMap<Option<String>, Box<dyn CliTask + Send + 'static>>,
}

impl Cli {
    #[must_use]
    pub(crate) fn new() -> Self {
        let default_task = Self::default_task();
        let command = default_task.subcommand();

        let mut tasks: HashMap<Option<String>, Box<dyn CliTask + Send + 'static>> = HashMap::new();
        tasks.insert(None, Box::new(default_task));

        let mut cli = Self { command, tasks };
        cli.add_task(CollectStatic);

        cli
    }

    pub(crate) fn set_metadata(&mut self, metadata: CliMetadata) {
        let mut command = std::mem::take(&mut self.command);
        command = command.name(metadata.name).version(metadata.version);

        if !metadata.authors.is_empty() {
            command = command.author(metadata.authors);
        }

        if !metadata.description.is_empty() {
            command = command.about(metadata.description);
        }

        self.command = command;
    }

    #[must_use]
    fn default_task() -> impl CliTask {
        RunServer
    }

    pub(crate) fn add_task<C>(&mut self, task: C)
    where
        C: CliTask + Send + 'static,
    {
        let subcommand = task.subcommand();
        let name = subcommand.get_name();

        assert!(
            !self.tasks.contains_key(&Some(name.to_owned())),
            "Task with name {name} already exists"
        );

        let name = name.to_owned();
        self.command = std::mem::take(&mut self.command).subcommand(subcommand);
        self.tasks.insert(Some(name), Box::new(task));
    }

    pub(crate) async fn execute(mut self, project: CotProject) -> Result<()> {
        let matches = self.command.get_matches();

        let subcommand_name = matches.subcommand_name();
        let task = self.tasks.get_mut(&subcommand_name.map(ToOwned::to_owned));

        let matches = match subcommand_name {
            Some(name) => matches.subcommand_matches(name).unwrap(),
            None => &matches,
        };

        task.expect("subcommand should exist if get_matches() didn't fail")
            .execute(matches, project)
            .await
    }
}

impl Default for Cli {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata for the CLI application.
///
/// This struct is used to set the name, version, authors, and description of
/// the CLI application. This is meant to be typically used in
/// [`crate::CotProjectBuilder::with_cli`] and [`metadata!`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct CliMetadata {
    /// The name that will be shown in the help message.
    pub name: &'static str,
    /// The version that will be shown when the `--version` flag is used.
    pub version: &'static str,
    /// The authors of the CLI application.
    pub authors: &'static str,
    /// The description that will be shown in the help message.
    pub description: &'static str,
}

/// A trait for defining a CLI command.
///
/// This is meant to be used with [`crate::CotProjectBuilder::add_task`].
#[async_trait]
pub trait CliTask {
    /// Returns the definition of the task's options as the [`clap`] crate's
    /// [`Command`].
    fn subcommand(&self) -> Command;

    /// Executes the task with the given matches and project.
    async fn execute(&mut self, matches: &ArgMatches, project: CotProject) -> Result<()>;
}

struct RunServer;

#[async_trait]
impl CliTask for RunServer {
    fn subcommand(&self) -> Command {
        Command::default().arg(
            Arg::new(LISTEN_PARAM)
                .help("Optional port to listen on, or address:port")
                .short('l')
                .long("listen")
                .default_value("127.0.0.1:8000")
                .value_name("ADDRPORT")
                .required(false),
        )
    }

    async fn execute(&mut self, matches: &ArgMatches, project: CotProject) -> Result<()> {
        let addr_port = matches
            .get_one::<String>(LISTEN_PARAM)
            .expect("default provided");

        let addr_port = if let Ok(port) = u16::from_str(addr_port) {
            format!("127.0.0.1:{port}")
        } else {
            addr_port.to_owned()
        };

        crate::run(project, &addr_port).await
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct CollectStatic;

#[async_trait]
impl CliTask for CollectStatic {
    fn subcommand(&self) -> Command {
        Command::new(COLLECT_STATIC_SUBCOMMAND)
            .about("Collects all static files into a static directory")
            .arg(
                Arg::new(COLLECT_STATIC_DIR_PARAM)
                    .help("The directory to collect the static files into")
                    .value_parser(value_parser!(PathBuf))
                    .required(true),
            )
    }

    async fn execute(&mut self, matches: &ArgMatches, project: CotProject) -> Result<()> {
        let dir = matches
            .get_one::<PathBuf>(COLLECT_STATIC_DIR_PARAM)
            .expect("required argument");
        println!("Collecting static files into {:?}", dir);

        StaticFiles::from(&project.context)
            .collect_into(dir)
            .map_err(|e| Error::new(ErrorRepr::CollectStatic { source: e }))?;

        Ok(())
    }
}

/// A macro to generate a [`CliMetadata`] struct from the Cargo manifest.
#[macro_export]
macro_rules! metadata {
    () => {{
        $crate::cli::CliMetadata {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            authors: env!("CARGO_PKG_AUTHORS"),
            description: env!("CARGO_PKG_DESCRIPTION"),
        }
    }};
}

pub use metadata;

use crate::static_files::StaticFiles;

#[cfg(test)]
mod tests {

    use bytes::Bytes;
    use clap::Command;
    use tempfile::tempdir;
    use thiserror::__private::AsDisplay;

    use super::*;
    use crate::CotApp;

    #[test]
    fn cli_new() {
        let cli = Cli::new();
        assert!(cli.command.get_name().is_empty());
        assert!(cli.tasks.contains_key(&None));
    }

    #[test]
    fn cli_set_metadata() {
        let mut cli = Cli::new();
        let metadata = CliMetadata {
            name: "test_app",
            version: "1.0",
            authors: "Author",
            description: "Test application",
        };
        cli.set_metadata(metadata);

        assert_eq!(cli.command.get_name(), "test_app");
        assert_eq!(cli.command.get_version().unwrap(), "1.0");
        assert_eq!(cli.command.get_author().unwrap(), "Author");
        assert_eq!(
            cli.command.get_about().unwrap().as_display().to_string(),
            "Test application"
        );
    }

    #[test]
    fn cli_add_task() {
        struct MyTask;

        #[async_trait]
        impl CliTask for MyTask {
            fn subcommand(&self) -> Command {
                Command::new("my-task")
            }

            async fn execute(&mut self, _matches: &ArgMatches, _project: CotProject) -> Result<()> {
                Ok(())
            }
        }

        let mut cli = Cli::new();
        cli.add_task(MyTask);

        assert!(cli.tasks.contains_key(&Some("my-task".to_owned())));
        assert!(cli
            .command
            .get_subcommands()
            .any(|sc| sc.get_name() == "my-task"));
    }

    #[test]
    fn run_server_subcommand() {
        let matches = RunServer
            .subcommand()
            .try_get_matches_from(vec!["test", "-l", "1024"]);

        assert!(matches.is_ok());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
    async fn collect_static_execute() {
        let mut collect_static = CollectStatic;
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().join("static").clone();

        struct App;

        impl CotApp for App {
            fn name(&self) -> &'static str {
                "test_app"
            }

            fn static_files(&self) -> Vec<(String, Bytes)> {
                vec![("test.txt".to_owned(), Bytes::from_static(b"test"))]
            }
        }

        let matches = CollectStatic
            .subcommand()
            .get_matches_from(vec!["test", temp_path.to_str().unwrap()]);

        let project = CotProject::builder()
            .register_app(App)
            .build()
            .await
            .unwrap();
        let result = collect_static.execute(&matches, project).await;

        assert!(result.is_ok());
        assert!(temp_path.join("test.txt").exists());
    }
}
