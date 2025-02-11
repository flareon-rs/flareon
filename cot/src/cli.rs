//! A command line interface for Cot-based applications.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
pub use clap;
use clap::{value_parser, Arg, ArgMatches, Command};
use derive_more::Debug;

use crate::error::ErrorRepr;
use crate::{Bootstrapper, Error, Result};

const CONFIG_PARAM: &str = "config";
const COLLECT_STATIC_SUBCOMMAND: &str = "collect-static";
const CHECK_SUBCOMMAND: &str = "check";
const LISTEN_PARAM: &str = "listen";
const COLLECT_STATIC_DIR_PARAM: &str = "dir";

#[derive(Debug)]
pub struct Cli {
    command: Command,
    #[debug("..")]
    tasks: HashMap<Option<String>, Box<dyn CliTask + Send + 'static>>,
}

impl Cli {
    #[must_use]
    pub(crate) fn new() -> Self {
        let default_task = Self::default_task();
        let command = default_task.subcommand();

        let command = command.arg(
            Arg::new(CONFIG_PARAM)
                .short('c')
                .long("config")
                .value_name("FILE")
                .default_value("dev")
                .help("Sets a custom config file"),
        );

        let mut tasks: HashMap<Option<String>, Box<dyn CliTask + Send + 'static>> = HashMap::new();
        tasks.insert(None, Box::new(default_task));

        let mut cli = Self { command, tasks };
        cli.add_task(Check);
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

    pub fn add_task<C>(&mut self, task: C)
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

    #[must_use]
    pub fn get_common_options(&mut self) -> CommonOptions {
        let matches = self.command.get_matches_mut();
        CommonOptions::new(matches)
    }

    #[allow(clippy::future_not_send)] // Send not needed; CLI is run async in a single thread
    pub(crate) async fn execute(mut self, bootstrapper: Bootstrapper<WithConfig>) -> Result<()> {
        let matches = self.command.get_matches();

        let subcommand_name = matches.subcommand_name();
        let task = self.tasks.get_mut(&subcommand_name.map(ToOwned::to_owned));

        let matches = match subcommand_name {
            Some(name) => matches.subcommand_matches(name).unwrap(),
            None => &matches,
        };

        task.expect("subcommand should exist if get_matches() didn't fail")
            .execute(matches, bootstrapper)
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
#[async_trait(?Send)]
pub trait CliTask {
    /// Returns the definition of the task's options as the [`clap`] crate's
    /// [`Command`].
    fn subcommand(&self) -> Command;

    /// Executes the task with the given matches and project.
    async fn execute(
        &mut self,
        matches: &ArgMatches,
        bootstrapper: Bootstrapper<WithConfig>,
    ) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonOptions {
    matches: ArgMatches,
}

impl CommonOptions {
    #[must_use]
    fn new(matches: ArgMatches) -> Self {
        Self { matches }
    }

    #[must_use]
    pub fn config(&self) -> &str {
        self.matches
            .get_one::<String>("config")
            .expect("default provided")
    }
}

struct RunServer;

#[async_trait(?Send)]
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

    async fn execute(
        &mut self,
        matches: &ArgMatches,
        bootstrapper: Bootstrapper<WithConfig>,
    ) -> Result<()> {
        let addr_port = matches
            .get_one::<String>(LISTEN_PARAM)
            .expect("default provided");

        let addr_port = if let Ok(port) = u16::from_str(addr_port) {
            format!("127.0.0.1:{port}")
        } else {
            addr_port.to_owned()
        };

        let bootstrapper = bootstrapper.boot().await?;
        crate::run(bootstrapper, &addr_port).await
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct CollectStatic;

#[async_trait(?Send)]
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

    async fn execute(
        &mut self,
        matches: &ArgMatches,
        bootstrapper: Bootstrapper<WithConfig>,
    ) -> Result<()> {
        let dir = matches
            .get_one::<PathBuf>(COLLECT_STATIC_DIR_PARAM)
            .expect("required argument");
        println!("Collecting static files into {:?}", dir);

        let bootstrapper = bootstrapper.with_apps();
        StaticFiles::from(bootstrapper.context())
            .collect_into(dir)
            .map_err(|e| Error::new(ErrorRepr::CollectStatic { source: e }))?;

        Ok(())
    }
}

struct Check;
#[async_trait(?Send)]
impl CliTask for Check {
    fn subcommand(&self) -> Command {
        Command::new(CHECK_SUBCOMMAND).about(
            "Verifies the configuration, including connections to the database and other services",
        )
    }

    async fn execute(
        &mut self,
        _matches: &ArgMatches,
        bootstrapper: Bootstrapper<WithConfig>,
    ) -> Result<()> {
        bootstrapper.boot().await?;
        println!("Success verifying the configuration");
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

use crate::project::WithConfig;
use crate::static_files::StaticFiles;

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use clap::Command;
    use cot::test::serial_guard;
    use tempfile::tempdir;
    use thiserror::__private::AsDisplay;

    use super::*;
    use crate::config::ProjectConfig;
    use crate::{App, AppBuilder, ProjectContext};

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

        #[async_trait(?Send)]
        impl CliTask for MyTask {
            fn subcommand(&self) -> Command {
                Command::new("my-task")
            }

            async fn execute(
                &mut self,
                _matches: &ArgMatches,
                _bootstrapper: Bootstrapper<WithConfig>,
            ) -> Result<()> {
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

        struct TestApp;
        impl App for TestApp {
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

        struct TestProject;
        impl cot::Project for TestProject {
            fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {
                apps.register(TestApp);
            }
        }

        let bootstrapper = Bootstrapper::new(TestProject).with_config(ProjectConfig::default());
        let result = collect_static.execute(&matches, bootstrapper).await;

        assert!(result.is_ok());
        assert!(temp_path.join("test.txt").exists());
    }

    #[tokio::test]
    async fn check_execute() {
        let config = r#"secret_key = "123abc""#;
        let result = test_check(config).await;

        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `geteuid` on OS `linux`
    #[cfg(feature = "db")]
    async fn check_execute_db_fail() {
        let config = r#"
        [database]
        url = "postgresql://invalid:invalid@invalid/invalid"
        "#;
        let result = test_check(config).await;

        assert!(result.is_err());
    }

    async fn test_check(config: &str) -> Result<()> {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config").clone();
        std::fs::create_dir_all(&config_path).unwrap();
        std::fs::write(config_path.join("test.toml"), config).unwrap();

        let mut check = Check;
        let matches = Check.subcommand().get_matches_from(Vec::<&str>::new());

        struct TestProject;
        impl cot::Project for TestProject {}

        // ensure the tests run sequentially when setting the current directory
        let _guard = serial_guard();

        std::env::set_current_dir(&temp_dir).unwrap();
        let bootstrapper = Bootstrapper::new(TestProject).with_config_name("test")?;
        check.execute(&matches, bootstrapper).await
    }
}
