//! This module contains the core types and traits for a Cot project.
//!
//! This module defines the [`Project`] and [`App`] traits, which are the main
//! entry points for your application.
/// # Examples
///
/// ```no_run
/// use cot::cli::CliMetadata;
/// use cot::Project;
///
/// struct MyProject;
/// impl Project for MyProject {
///     fn cli_metadata(&self) -> CliMetadata {
///         cot::cli::metadata!()
///     }
/// }
///
/// #[cot::main]
/// fn main() -> impl Project {
///     MyProject
/// }
/// ```
use std::future::poll_fn;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::handler::HandlerWithoutStateExt;
use bytes::Bytes;
use derive_more::with_trait::Debug;
use futures_util::FutureExt;
use http::request::Parts;
use tower::{Layer, Service};
use tracing::{error, info, trace};

use crate::admin::AdminModelManager;
#[cfg(feature = "db")]
use crate::auth::db::DatabaseUserBackend;
use crate::auth::{AuthBackend, NoAuthBackend};
use crate::cli::Cli;
#[cfg(feature = "db")]
use crate::config::DatabaseConfig;
use crate::config::{AuthBackendConfig, ProjectConfig};
#[cfg(feature = "db")]
use crate::db::migrations::{MigrationEngine, SyncDynMigration};
#[cfg(feature = "db")]
use crate::db::Database;
use crate::error::ErrorRepr;
use crate::error_page::{Diagnostics, ErrorPageTrigger};
use crate::handler::BoxedHandler;
use crate::middleware::{IntoCotError, IntoCotErrorLayer, IntoCotResponse, IntoCotResponseLayer};
use crate::request::{Request, RequestExt};
use crate::response::{Response, ResponseExt};
use crate::router::{Route, Router, RouterService};
use crate::{cli, error_page, Body, Error, StatusCode};

/// A building block for a Cot project.
///
/// A Cot app is a part (ideally, reusable) of a Cot project that is
/// responsible for its own set of functionalities. Examples of apps could be:
/// * admin panel
/// * user authentication
/// * blog
/// * message board
/// * session management
/// * etc.
///
/// Each app can have its own set of URLs that it can handle which can be
/// mounted on the project's router, its own set of middleware, database
/// migrations (which can depend on other apps), etc.
#[async_trait]
pub trait App: Send + Sync {
    /// The name of the app.
    ///
    /// This should usually be the name of the crate.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::App;
    ///
    /// struct MyApp;
    /// impl App for MyApp {
    ///     fn name(&self) -> &str {
    ///         env!("CARGO_PKG_NAME")
    ///     }
    /// }
    /// ```
    fn name(&self) -> &str;

    /// Initializes the app.
    ///
    /// This method is called when the app is initialized. It can be used to
    /// initialize whatever is needed for the app to work, possibly depending on
    /// other apps, or the project's configuration.
    ///
    /// # Errors
    ///
    /// This method returns an error if the app fails to initialize.
    #[allow(unused_variables)]
    async fn init(&self, context: &mut ProjectContext) -> crate::Result<()> {
        Ok(())
    }

    /// Returns the router for the app. By default, it returns an empty router.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::Request;
    /// use cot::response::{Response, ResponseExt};
    /// use cot::router::{Route, Router};
    /// use cot::{App, Body, StatusCode};
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     Ok(Response::new_html(
    ///         StatusCode::OK,
    ///         Body::fixed("Hello world!"),
    ///     ))
    /// }
    ///
    /// struct MyApp;
    /// impl App for MyApp {
    ///     fn name(&self) -> &str {
    ///         "my_app"
    ///     }
    ///
    ///     fn router(&self) -> Router {
    ///         Router::with_urls([Route::with_handler("/", index)])
    ///     }
    /// }
    /// ```
    fn router(&self) -> Router {
        Router::empty()
    }

    /// Returns the migrations for the app. By default, it returns an empty
    /// list.
    #[cfg(feature = "db")]
    fn migrations(&self) -> Vec<Box<SyncDynMigration>> {
        vec![]
    }

    /// Returns the admin model managers for the app. By default, it returns an
    /// empty list.
    fn admin_model_managers(&self) -> Vec<Box<dyn AdminModelManager>> {
        vec![]
    }

    /// Returns a list of static files that the app serves. By default, it
    /// returns an empty list.
    fn static_files(&self) -> Vec<(String, Bytes)> {
        vec![]
    }
}

/// The main trait for a Cot project.
///
/// This is the main entry point for your application. This trait defines
/// the configuration, apps, and other project-wide resources.
///
/// It's mainly meant to be used with the [`cot::main`] attribute macro.
///
/// # Examples
///
/// ```no_run
/// use cot::cli::CliMetadata;
/// use cot::Project;
///
/// struct MyProject;
/// impl Project for MyProject {
///     fn cli_metadata(&self) -> CliMetadata {
///         cot::cli::metadata!()
///     }
/// }
///
/// #[cot::main]
/// fn main() -> impl Project {
///     MyProject
/// }
/// ```
pub trait Project {
    /// Returns the metadata for the CLI.
    ///
    /// This method is used to set the name, version, authors, and description
    /// of the CLI application. This is meant to be typically used with
    /// [`cli::metadata!()`] which automatically retrieves this data from the
    /// crate metadata.
    ///
    /// The default implementation sets the name, version, authors, and
    /// description of the `cot` crate.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::cli::CliMetadata;
    /// use cot::Project;
    ///
    /// struct HelloProject;
    /// impl Project for HelloProject {
    ///     fn cli_metadata(&self) -> CliMetadata {
    ///         cot::cli::metadata!()
    ///     }
    /// }
    /// ```
    fn cli_metadata(&self) -> cli::CliMetadata {
        cli::metadata!()
    }

    /// Returns the configuration for the project.
    ///
    /// The default implementation reads the configuration from the `config`
    /// directory in the current working directory (for instance, if
    /// `config_name` is `test`, then `config/test.toml` in the current working
    /// directory is read). If the file does not exist, it tries to read the
    /// file directly at `config_name` path.
    ///
    /// You might want to override this method if you want to read the
    /// configuration from a different source, or if you want to hardcode
    /// it in the binary.
    ///
    /// # Errors
    ///
    /// This method may return an error if it cannot read or parse the
    /// configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::Project;
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn config(&self, config_name: &str) -> cot::Result<ProjectConfig> {
    ///         Ok(ProjectConfig::default())
    ///     }
    /// }
    /// ```
    fn config(&self, config_name: &str) -> crate::Result<ProjectConfig> {
        read_config(config_name)
    }

    /// Adds a task to the CLI.
    ///
    /// This method is used to add a task to the CLI. The task will be available
    /// as a subcommand of the main CLI command.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_trait::async_trait;
    /// use clap::{ArgMatches, Command};
    /// use cot::cli::{Cli, CliTask};
    /// use cot::project::WithConfig;
    /// use cot::{Bootstrapper, Project};
    ///
    /// struct Frobnicate;
    ///
    /// #[async_trait(?Send)]
    /// impl CliTask for Frobnicate {
    ///     fn subcommand(&self) -> Command {
    ///         Command::new("frobnicate")
    ///     }
    ///
    ///     async fn execute(
    ///         &mut self,
    ///         _matches: &ArgMatches,
    ///         _bootstrapper: Bootstrapper<WithConfig>,
    ///     ) -> cot::Result<()> {
    ///         println!("Frobnicating...");
    ///
    ///         Ok(())
    ///     }
    /// }
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn register_tasks(&self, cli: &mut Cli) {
    ///         cli.add_task(Frobnicate)
    ///     }
    /// }
    /// ```
    #[allow(unused_variables)]
    fn register_tasks(&self, cli: &mut Cli) {}

    /// Registers the apps for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::{AppBuilder, WithConfig};
    /// use cot::{App, Project, ProjectContext};
    ///
    /// struct MyApp;
    /// impl App for MyApp {
    ///     fn name(&self) -> &str {
    ///         "my_app"
    ///     }
    /// }
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {
    ///         apps.register(MyApp);
    ///     }
    /// }
    /// ```
    #[allow(unused_variables)]
    fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {}

    /// Sets the authentication backend to use.
    ///
    /// Note that it's typically not necessary to override this method, as it
    /// already provides a default implementation that uses the auth backend
    /// specified in the project's configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::auth::{AuthBackend, NoAuthBackend};
    /// use cot::project::WithApps;
    /// use cot::{App, Project, ProjectContext};
    ///
    /// struct HelloProject;
    /// impl Project for HelloProject {
    ///     fn auth_backend(&self, app_context: &ProjectContext<WithApps>) -> Box<dyn AuthBackend> {
    ///         Box::new(NoAuthBackend)
    ///     }
    /// }
    /// ```
    fn auth_backend(&self, app_context: &ProjectContext<WithApps>) -> Box<dyn AuthBackend> {
        #[allow(trivial_casts)] // cast to Box<dyn AuthBackend>
        match &app_context.config().auth_backend {
            AuthBackendConfig::None => Box::new(NoAuthBackend) as Box<dyn AuthBackend>,
            #[cfg(feature = "db")]
            AuthBackendConfig::Database => Box::new(DatabaseUserBackend) as Box<dyn AuthBackend>,
        }
    }

    /// Returns the middlewares for the project.
    ///
    /// This method is used to return the middlewares for the project. The
    /// middlewares will be applied to all routes in the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::middleware::LiveReloadMiddleware;
    /// use cot::project::{RootHandlerBuilder, WithApps};
    /// use cot::{BoxedHandler, Project, ProjectContext};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn middlewares(
    ///         &self,
    ///         handler: RootHandlerBuilder,
    ///         context: &ProjectContext<WithApps>,
    ///     ) -> BoxedHandler {
    ///         handler
    ///             .middleware(LiveReloadMiddleware::from_app_context(context))
    ///             .build()
    ///     }
    /// }
    /// ```
    #[allow(unused_variables)]
    fn middlewares(
        &self,
        handler: RootHandlerBuilder,
        context: &ProjectContext<WithApps>,
    ) -> BoxedHandler {
        handler.build()
    }

    /// Returns the 500 Internal Server Error handler for the project.
    ///
    /// The default handler returns a simple, static page.
    ///
    /// # Errors
    ///
    /// This method may return an error if the handler fails to build a
    /// response. In this case, the error will be logged and a generic
    /// error page will be returned to the user.
    ///
    /// # Panics
    ///
    /// Note that this handler is exempt of the typical panic handling
    /// machinery in Cot. This means that if this handler panics, no
    /// response will be sent to a user. Because of that, you should
    /// avoid panicking here and return [`Err`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::ErrorPageHandler;
    /// use cot::response::{Response, ResponseExt};
    /// use cot::{Body, Project, StatusCode};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn server_error_handler(&self) -> Box<dyn ErrorPageHandler> {
    ///         Box::new(MyHandler)
    ///     }
    /// }
    ///
    /// struct MyHandler;
    /// impl ErrorPageHandler for MyHandler {
    ///     fn handle(&self) -> cot::Result<Response> {
    ///         Ok(Response::new_html(
    ///             StatusCode::INTERNAL_SERVER_ERROR,
    ///             Body::fixed("Internal Server Error"),
    ///         ))
    ///     }
    /// }
    /// ```
    fn server_error_handler(&self) -> Box<dyn ErrorPageHandler> {
        Box::new(DefaultServerErrorHandler)
    }

    /// Returns the 404 Not Found handler for the project.
    ///
    /// The default handler returns a simple, static page.
    ///
    /// # Errors
    ///
    /// This method may return an error if the handler fails to build a
    /// response. In this case, the error will be logged and a generic
    /// error page will be returned to the user.
    ///
    /// # Panics
    ///
    /// Note that this handler is exempt of the typical panic handling
    /// machinery in Cot. This means that if this handler panics, no
    /// response will be sent to a user. Because of that, you should
    /// avoid panicking here and return [`Err`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::ErrorPageHandler;
    /// use cot::response::{Response, ResponseExt};
    /// use cot::{Body, Project, StatusCode};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn not_found_handler(&self) -> Box<dyn ErrorPageHandler> {
    ///         Box::new(MyHandler)
    ///     }
    /// }
    ///
    /// struct MyHandler;
    /// impl ErrorPageHandler for MyHandler {
    ///     fn handle(&self) -> cot::Result<Response> {
    ///         Ok(Response::new_html(
    ///             StatusCode::NOT_FOUND,
    ///             Body::fixed("Not Found"),
    ///         ))
    ///     }
    /// }
    /// ```
    fn not_found_handler(&self) -> Box<dyn ErrorPageHandler> {
        Box::new(DefaultNotFoundHandler)
    }
}

/// A helper struct to build the root handler for the project.
///
/// This is mainly useful for attaching middlewares to the project.
///
/// # Examples
///
/// ```
/// use cot::middleware::LiveReloadMiddleware;
/// use cot::project::{RootHandlerBuilder, WithApps};
/// use cot::{BoxedHandler, Project, ProjectContext};
///
/// struct MyProject;
/// impl Project for MyProject {
///     fn middlewares(
///         &self,
///         handler: RootHandlerBuilder,
///         context: &ProjectContext<WithApps>,
///     ) -> BoxedHandler {
///         handler
///             .middleware(LiveReloadMiddleware::from_app_context(context))
///             .build()
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RootHandlerBuilder<S = RouterService> {
    handler: S,
}

impl<S> RootHandlerBuilder<S>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
    S::Future: Send,
{
    /// Adds middleware to the project.
    ///
    /// This method is used to add middleware to the project. The middleware
    /// will be applied to all routes in the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::middleware::LiveReloadMiddleware;
    /// use cot::project::{RootHandlerBuilder, WithApps};
    /// use cot::{BoxedHandler, Project, ProjectContext};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn middlewares(
    ///         &self,
    ///         handler: RootHandlerBuilder,
    ///         context: &ProjectContext<WithApps>,
    ///     ) -> BoxedHandler {
    ///         handler
    ///             .middleware(LiveReloadMiddleware::from_app_context(context))
    ///             .build()
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn middleware<M>(
        self,
        middleware: M,
    ) -> RootHandlerBuilder<IntoCotError<IntoCotResponse<<M as Layer<S>>::Service>>>
    where
        M: Layer<S>,
    {
        let layer = (
            IntoCotErrorLayer::new(),
            IntoCotResponseLayer::new(),
            middleware,
        );

        RootHandlerBuilder {
            handler: layer.layer(self.handler),
        }
    }

    /// Builds the root handler for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::middleware::LiveReloadMiddleware;
    /// use cot::project::{RootHandlerBuilder, WithApps};
    /// use cot::{BoxedHandler, Project, ProjectContext};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn middlewares(
    ///         &self,
    ///         handler: RootHandlerBuilder,
    ///         context: &ProjectContext<WithApps>,
    ///     ) -> BoxedHandler {
    ///         handler
    ///             .middleware(LiveReloadMiddleware::from_app_context(context))
    ///             .build()
    ///     }
    /// }
    /// ```
    pub fn build(self) -> BoxedHandler {
        BoxedHandler::new(self.handler)
    }
}

/// A helper struct to build the apps for the project.
///
/// # Examples
///
/// ```
/// use cot::project::{AppBuilder, WithConfig};
/// use cot::{App, Project, ProjectContext};
///
/// struct MyApp;
/// impl App for MyApp {
///     fn name(&self) -> &str {
///         "my_app"
///     }
/// }
///
/// struct MyProject;
/// impl Project for MyProject {
///     fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {
///         apps.register(MyApp);
///     }
/// }
/// ```
#[derive(Debug)]
pub struct AppBuilder {
    #[debug("..")]
    apps: Vec<Box<dyn App>>,
    urls: Vec<Route>,
}

impl AppBuilder {
    fn new() -> Self {
        Self {
            apps: Vec::new(),
            urls: Vec::new(),
        }
    }

    /// Registers an app.
    ///
    /// This method is used to register an app. The app's views, if any, will
    /// not be available.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::WithConfig;
    /// use cot::{App, Project};
    ///
    /// struct HelloApp;
    ///
    /// impl App for HelloApp {
    ///     fn name(&self) -> &'static str {
    ///         env!("CARGO_PKG_NAME")
    ///     }
    /// }
    ///
    /// struct HelloProject;
    /// impl Project for HelloProject {
    ///     fn register_apps(
    ///         &self,
    ///         apps: &mut cot::AppBuilder,
    ///         _context: &cot::ProjectContext<WithConfig>,
    ///     ) {
    ///         apps.register(HelloApp);
    ///     }
    /// }
    /// ```
    pub fn register<T: App + 'static>(&mut self, module: T) {
        self.apps.push(Box::new(module));
    }

    /// Registers an app with views.
    ///
    /// This method is used to register an app with views. The app's views will
    /// be available at the given URL prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::WithConfig;
    /// use cot::{App, Project};
    ///
    /// struct HelloApp;
    ///
    /// impl App for HelloApp {
    ///     fn name(&self) -> &'static str {
    ///         env!("CARGO_PKG_NAME")
    ///     }
    /// }
    ///
    /// struct HelloProject;
    /// impl Project for HelloProject {
    ///     fn register_apps(
    ///         &self,
    ///         apps: &mut cot::AppBuilder,
    ///         _context: &cot::ProjectContext<WithConfig>,
    ///     ) {
    ///         apps.register_with_views(HelloApp, "/hello");
    ///     }
    /// }
    /// ```
    pub fn register_with_views<T: App + 'static>(&mut self, module: T, url_prefix: &str) {
        self.urls
            .push(Route::with_router(url_prefix, module.router()));
        self.register(module);
    }
}

/// A trait for defining custom error page handlers.
///
/// This is useful with [`Project::server_error_handler`] and
/// [`Project::not_found_handler`].
///
/// # Examples
///
/// ```
/// use cot::project::ErrorPageHandler;
/// use cot::response::{Response, ResponseExt};
/// use cot::{Body, Project, StatusCode};
///
/// struct MyProject;
/// impl Project for MyProject {
///     fn not_found_handler(&self) -> Box<dyn ErrorPageHandler> {
///         Box::new(MyHandler)
///     }
/// }
///
/// struct MyHandler;
/// impl ErrorPageHandler for MyHandler {
///     fn handle(&self) -> cot::Result<Response> {
///         Ok(Response::new_html(
///             StatusCode::NOT_FOUND,
///             Body::fixed("Not Found"),
///         ))
///     }
/// }
/// ```
pub trait ErrorPageHandler: Send + Sync {
    /// Returns the error response.
    ///
    /// # Errors
    ///
    /// This method may return an error if the handler fails to build a
    /// response. In this case, the error will be logged and a generic
    /// error page will be returned to the user.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::ErrorPageHandler;
    /// use cot::response::{Response, ResponseExt};
    /// use cot::{Body, Project, StatusCode};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn not_found_handler(&self) -> Box<dyn ErrorPageHandler> {
    ///         Box::new(MyHandler)
    ///     }
    /// }
    ///
    /// struct MyHandler;
    /// impl ErrorPageHandler for MyHandler {
    ///     fn handle(&self) -> cot::Result<Response> {
    ///         Ok(Response::new_html(
    ///             StatusCode::NOT_FOUND,
    ///             Body::fixed("Not Found"),
    ///         ))
    ///     }
    /// }
    /// ```
    fn handle(&self) -> crate::Result<Response>;
}

struct DefaultNotFoundHandler;
impl ErrorPageHandler for DefaultNotFoundHandler {
    fn handle(&self) -> crate::Result<Response> {
        Ok(Response::new_html(
            StatusCode::NOT_FOUND,
            Body::fixed(include_str!("../templates/404.html")),
        ))
    }
}

struct DefaultServerErrorHandler;
impl ErrorPageHandler for DefaultServerErrorHandler {
    fn handle(&self) -> crate::Result<Response> {
        Ok(Response::new_html(
            StatusCode::INTERNAL_SERVER_ERROR,
            Body::fixed(include_str!("../templates/500.html")),
        ))
    }
}

/// The main struct for bootstrapping the project.
///
/// This is the core struct for bootstrapping the project. It goes over the
/// different phases of bootstrapping the project which are defined in the
/// [`BootstrapPhase`] trait. Each phase has its own subset of the project's
/// context that is available, and you have access to specific parts of the
/// project's context depending where you are in the bootstrapping process.
///
/// Note that you shouldn't have to use this struct directly most of the time.
/// It's mainly used internally by the `cot` crate to bootstrap the project.
/// It can be useful if you want to control the bootstrapping process in
/// custom [`CliTask`](cli::CliTask)s.
///
/// # Examples
///
/// ```
/// use cot::project::{Bootstrapper, WithConfig};
/// use cot::{App, Project};
///
/// struct MyProject;
/// impl Project for MyProject {}
///
/// # #[tokio::main]
/// # async fn main() -> cot::Result<()> {
/// let bootstrapper = Bootstrapper::new(MyProject)
///     .with_config(cot::config::ProjectConfig::default())
///     .boot()
///     .await?;
/// let (context, handler) = bootstrapper.into_context_and_handler();
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Bootstrapper<S: BootstrapPhase = Initialized> {
    #[debug("..")]
    project: Box<dyn Project>,
    context: ProjectContext<S>,
    handler: S::RequestHandler,
}

impl Bootstrapper<Uninitialized> {
    /// Creates a new bootstrapper.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::{Bootstrapper, WithConfig};
    /// use cot::{App, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new<P: Project + 'static>(project: P) -> Self {
        Self {
            project: Box::new(project),
            context: ProjectContext::new(),
            handler: (),
        }
    }
}

impl<S: BootstrapPhase> Bootstrapper<S> {
    /// Returns the project for the bootstrapper.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::{Bootstrapper, WithConfig};
    /// use cot::{App, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject);
    /// # Ok(())
    /// # }
    /// ```
    pub fn project(&self) -> &dyn Project {
        self.project.as_ref()
    }

    /// Returns the app context for the bootstrapper.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::{Bootstrapper, WithConfig};
    /// use cot::{App, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject);
    /// let context = bootstrapper.app_context();
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn app_context(&self) -> &ProjectContext<S> {
        &self.context
    }

    /// Returns the context for the bootstrapper.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::project::{Bootstrapper, WithConfig};
    /// use cot::{App, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject);
    /// let context = bootstrapper.context();
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn context(&self) -> &ProjectContext<S> {
        &self.context
    }
}

impl Bootstrapper<Uninitialized> {
    #[allow(clippy::future_not_send)] // Send not needed; CLI is run async in a single thread
    async fn run_cli(self) -> cot::Result<()> {
        let mut cli = Cli::new();

        cli.set_metadata(self.project.cli_metadata());
        self.project.register_tasks(&mut cli);

        let common_options = cli.get_common_options();
        let self_with_context = self.with_config_name(common_options.config())?;

        cli.execute(self_with_context).await
    }

    /// Reads the configuration of the project and moves to the next
    /// bootstrapping phase.
    ///
    /// # Errors
    ///
    /// This method may return an error if it cannot read the configuration of
    /// the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::{Bootstrapper, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {
    ///     fn config(&self, config_name: &str) -> cot::Result<ProjectConfig> {
    ///         Ok(ProjectConfig::default())
    ///     }
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject)
    ///     .with_config_name("test")?
    ///     .boot()
    ///     .await?;
    /// let (context, handler) = bootstrapper.into_context_and_handler();
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_config_name(self, config_name: &str) -> cot::Result<Bootstrapper<WithConfig>> {
        let config = self.project.config(config_name)?;

        Ok(self.with_config(config))
    }

    /// Sets the configuration for the project.
    ///
    /// This is mainly useful in tests, where you want to override the default
    /// behavior of reading the configuration from a file.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::{Bootstrapper, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject)
    ///     .with_config(ProjectConfig::default())
    ///     .boot()
    ///     .await?;
    /// let (context, handler) = bootstrapper.into_context_and_handler();
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_config(self, config: ProjectConfig) -> Bootstrapper<WithConfig> {
        Bootstrapper {
            project: self.project,
            context: self.context.with_config(config),
            handler: self.handler,
        }
    }
}

fn read_config(config: &str) -> cot::Result<ProjectConfig> {
    trace!(config, "Reading project configuration");
    let result = match std::fs::read_to_string(config) {
        Ok(config_content) => Ok(config_content),
        Err(_err) => {
            // try to read the config from the `config` directory if it's not a file
            let path = PathBuf::from("config").join(config).with_extension("toml");
            trace!(
                config,
                path = %path.display(),
                "Failed to read config as a file; trying to read from the `config` directory"
            );

            std::fs::read_to_string(&path)
        }
    };

    let config_content = result.map_err(|err| {
        Error::new(ErrorRepr::LoadConfig {
            config: config.to_owned(),
            source: err,
        })
    })?;

    ProjectConfig::from_toml(&config_content)
}

impl Bootstrapper<WithConfig> {
    /// Builds the Cot project instance.
    ///
    /// This is the final step in the bootstrapping process. It initializes the
    /// project with the given configuration and returns a [`Bootstrapper`]
    /// instance that contains the project's context and handler.
    ///
    /// You shouldn't have to use this method directly most of the time. It's
    /// mainly useful for controlling the bootstrapping process in custom
    /// [`CliTask`](cli::CliTask)s.
    ///
    /// # Errors
    ///
    /// This method may return an error if it cannot initialize any of the
    /// project's components, such as the database.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::{Bootstrapper, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject)
    ///     .with_config(ProjectConfig::default())
    ///     .boot()
    ///     .await?;
    /// let (context, handler) = bootstrapper.into_context_and_handler();
    /// # Ok(())
    /// # }
    /// ```
    // Send not needed; Bootstrapper is run async in a single thread
    #[allow(clippy::future_not_send)]
    pub async fn boot(self) -> cot::Result<Bootstrapper<Initialized>> {
        self.with_apps().boot().await
    }

    /// Moves forward to the next phase of bootstrapping, the with-apps phase.
    ///
    /// See the [`BootstrapPhase`] and [`WithApps`] documentation for more
    /// details.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::project::{Bootstrapper, WithApps};
    /// use cot::{AppBuilder, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject)
    ///     .with_config(ProjectConfig::default())
    ///     .with_apps()
    ///     .boot()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_apps(self) -> Bootstrapper<WithApps> {
        let mut module_builder = AppBuilder::new();
        self.project
            .register_apps(&mut module_builder, &self.context);

        let router = Arc::new(Router::with_urls(module_builder.urls));

        let app_context = self.context.with_apps(module_builder.apps, router);

        Bootstrapper {
            project: self.project,
            context: app_context,
            handler: self.handler,
        }
    }
}

impl Bootstrapper<WithApps> {
    /// Builds the Cot project instance.
    ///
    /// This is the final step in the bootstrapping process. It initializes the
    /// project with the given configuration and returns a [`Bootstrapper`]
    /// instance that contains the project's context and handler.
    ///
    /// You shouldn't have to use this method directly most of the time. It's
    /// mainly useful for controlling the bootstrapping process in custom
    /// [`CliTask`](cli::CliTask)s.
    ///
    /// # Errors
    ///
    /// This method may return an error if it cannot initialize any of the
    /// project's components, such as the database.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::{Bootstrapper, Project};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject)
    ///     .with_config(ProjectConfig::default())
    ///     .boot()
    ///     .await?;
    /// let (context, handler) = bootstrapper.into_context_and_handler();
    /// # Ok(())
    /// # }
    /// ```
    // Send not needed; Bootstrapper is run async in a single thread
    #[allow(clippy::future_not_send)]
    pub async fn boot(self) -> cot::Result<Bootstrapper<Initialized>> {
        let router_service = RouterService::new(Arc::clone(&self.context.router));
        let handler = RootHandlerBuilder {
            handler: router_service,
        };
        let handler = self.project.middlewares(handler, &self.context);

        let auth_backend = self.project.auth_backend(&self.context);
        #[cfg(feature = "db")]
        let database = Self::init_database(&self.context.config.database).await?;
        let app_context = self.context.with_auth_and_db(
            auth_backend,
            #[cfg(feature = "db")]
            database,
        );

        Ok(Bootstrapper {
            project: self.project,
            context: app_context,
            handler,
        })
    }

    #[cfg(feature = "db")]
    async fn init_database(config: &DatabaseConfig) -> cot::Result<Option<Arc<Database>>> {
        match &config.url {
            Some(url) => {
                let database = Database::new(url.as_str()).await?;
                Ok(Some(Arc::new(database)))
            }
            None => Ok(None),
        }
    }
}

impl Bootstrapper<Initialized> {
    /// Returns the context and handler of the bootstrapper.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::project::Bootstrapper;
    /// use cot::{Project, ProjectContext};
    ///
    /// struct MyProject;
    /// impl Project for MyProject {}
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let bootstrapper = Bootstrapper::new(MyProject)
    ///     .with_config(ProjectConfig::default())
    ///     .boot()
    ///     .await?;
    /// let (context, handler) = bootstrapper.into_context_and_handler();
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn into_context_and_handler(self) -> (ProjectContext, BoxedHandler) {
        (self.context, self.handler)
    }
}

mod sealed {
    pub trait Sealed {}
}

/// A trait that represents the different phases of the bootstrapper.
///
/// This trait is used to define the types for the different phases of the
/// bootstrapper. It's used to ensure that you can't access nonexistent
/// data until the bootstrapper has reached the corresponding phase.
///
/// # Sealed
///
/// This trait is sealed and can't be implemented outside the `cot`
/// crate.
///
/// # Examples
///
/// ```
/// ///
/// use cot::project::{RootHandlerBuilder, WithApps, WithConfig};
/// use cot::{AppBuilder, BoxedHandler, Project, ProjectContext};
///
/// struct MyProject;
/// impl Project for MyProject {
///     // `WithConfig` phase here
///     fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {
///         todo!();
///     }
///
///     // `WithApps` phase here (which comes after `WithConfig`)
///     fn middlewares(
///         &self,
///         handler: RootHandlerBuilder,
///         context: &ProjectContext<WithApps>,
///     ) -> BoxedHandler {
///         todo!()
///     }
/// }
/// ```
pub trait BootstrapPhase: sealed::Sealed {
    // Bootstrapper types
    /// The type of the request handler.
    type RequestHandler: Debug;

    // App context types
    /// The type of the configuration.
    type Config: Debug;
    /// The type of the apps.
    type Apps;
    /// The type of the router.
    type Router: Debug;
    /// The type of the auth backend.
    type AuthBackend;
    /// The type of the database.
    #[cfg(feature = "db")]
    type Database: Debug;
}

/// First phase of bootstrapping a Cot project, the uninitialized phase.
///
/// # See also
///
/// See the details about the different bootstrap phases in the
/// [`BootstrapPhase`] trait documentation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Uninitialized;

impl sealed::Sealed for Uninitialized {}
impl BootstrapPhase for Uninitialized {
    type RequestHandler = ();
    type Config = ();
    type Apps = ();
    type Router = ();
    type AuthBackend = ();
    #[cfg(feature = "db")]
    type Database = ();
}

/// Second phase of bootstrapping a Cot project, the with-config phase.
///
/// # See also
///
/// See the details about the different bootstrap phases in the
/// [`BootstrapPhase`] trait documentation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct WithConfig;

impl sealed::Sealed for WithConfig {}
impl BootstrapPhase for WithConfig {
    type RequestHandler = ();
    type Config = Arc<ProjectConfig>;
    type Apps = ();
    type Router = ();
    type AuthBackend = ();
    #[cfg(feature = "db")]
    type Database = ();
}

/// Third phase of bootstrapping a Cot project, the with-apps phase.
///
/// # See also
///
/// See the details about the different bootstrap phases in the
/// [`BootstrapPhase`] trait documentation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct WithApps;

impl sealed::Sealed for WithApps {}
impl BootstrapPhase for WithApps {
    type RequestHandler = ();
    type Config = <WithConfig as BootstrapPhase>::Config;
    type Apps = Vec<Box<dyn App>>;
    type Router = Arc<Router>;
    type AuthBackend = ();
    #[cfg(feature = "db")]
    type Database = ();
}

/// The final phase of bootstrapping a Cot project, the initialized phase.
///
/// # See also
///
/// See the details about the different bootstrap phases in the
/// [`BootstrapPhase`] trait documentation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Initialized;

impl sealed::Sealed for Initialized {}
impl BootstrapPhase for Initialized {
    type RequestHandler = BoxedHandler;
    type Config = <WithApps as BootstrapPhase>::Config;
    type Apps = <WithApps as BootstrapPhase>::Apps;
    type Router = <WithApps as BootstrapPhase>::Router;
    type AuthBackend = Box<dyn AuthBackend>;
    #[cfg(feature = "db")]
    type Database = Option<Arc<Database>>;
}

/// Shared context and configs for all apps. Used in conjunction with the
/// [`Project`] trait.
#[derive(Debug)]
pub struct ProjectContext<S: BootstrapPhase = Initialized> {
    config: S::Config,
    #[debug("..")]
    apps: S::Apps,
    router: S::Router,
    #[debug("..")]
    auth_backend: S::AuthBackend,
    #[cfg(feature = "db")]
    database: S::Database,
}

impl ProjectContext<Uninitialized> {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            config: (),
            apps: (),
            router: (),
            auth_backend: (),
            #[cfg(feature = "db")]
            database: (),
        }
    }

    fn with_config(self, config: ProjectConfig) -> ProjectContext<WithConfig> {
        ProjectContext {
            config: Arc::new(config),
            apps: self.apps,
            router: self.router,
            auth_backend: self.auth_backend,
            #[cfg(feature = "db")]
            database: self.database,
        }
    }
}

impl ProjectContext<WithConfig> {
    /// Returns the configuration for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let config = request.context().config();
    ///     // can also be accessed via:
    ///     let config = request.project_config();
    ///
    ///     let db_url = &config.database.url;
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    #[must_use]
    fn with_apps(self, apps: Vec<Box<dyn App>>, router: Arc<Router>) -> ProjectContext<WithApps> {
        ProjectContext {
            config: self.config,
            apps,
            router,
            auth_backend: self.auth_backend,
            #[cfg(feature = "db")]
            database: self.database,
        }
    }
}

impl ProjectContext<WithApps> {
    /// Returns the configuration for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let config = request.context().config();
    ///     // can also be accessed via:
    ///     let config = request.project_config();
    ///
    ///     let db_url = &config.database.url;
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// Returns the apps for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let apps = request.context().apps();
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn apps(&self) -> &[Box<dyn App>] {
        &self.apps
    }

    #[must_use]
    fn with_auth_and_db(
        self,
        auth_backend: Box<dyn AuthBackend>,
        #[cfg(feature = "db")] database: Option<Arc<Database>>,
    ) -> ProjectContext<Initialized> {
        ProjectContext {
            config: self.config,
            apps: self.apps,
            router: self.router,
            auth_backend,
            #[cfg(feature = "db")]
            database,
        }
    }
}

impl ProjectContext<Initialized> {
    pub(crate) fn initialized(
        config: <Initialized as BootstrapPhase>::Config,
        apps: <Initialized as BootstrapPhase>::Apps,
        router: <Initialized as BootstrapPhase>::Router,
        auth_backend: <Initialized as BootstrapPhase>::AuthBackend,
        #[cfg(feature = "db")] database: <Initialized as BootstrapPhase>::Database,
    ) -> Self {
        Self {
            config,
            apps,
            router,
            auth_backend,
            #[cfg(feature = "db")]
            database,
        }
    }

    /// Returns the configuration for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let config = request.context().config();
    ///     // can also be accessed via:
    ///     let config = request.project_config();
    ///
    ///     let db_url = &config.database.url;
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// Returns the apps for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let apps = request.context().apps();
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn apps(&self) -> &[Box<dyn App>] {
        &self.apps
    }

    /// Returns the router for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let router = request.context().config();
    ///     // can also be accessed via:
    ///     let router = request.router();
    ///
    ///     let num_routes = router.routes().len();
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Returns the authentication backend for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let auth_backend = request.context().auth_backend();
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn auth_backend(&self) -> &dyn AuthBackend {
        self.auth_backend.as_ref()
    }

    /// Returns the database for the project, if it is enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let database = request.context().try_database();
    ///     if let Some(database) = database {
    ///         // do something with the database
    ///     } else {
    ///         // database is not enabled
    ///     }
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    #[cfg(feature = "db")]
    pub fn try_database(&self) -> Option<&Arc<Database>> {
        self.database.as_ref()
    }

    /// Returns the database for the project, if it is enabled.
    ///
    /// # Panics
    ///
    /// This method panics if the database is not enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let database = request.context().database();
    ///     // can also be accessed via:
    ///     request.db();
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    #[cfg(feature = "db")]
    pub fn database(&self) -> &Database {
        self.try_database().expect(
            "Database missing. Did you forget to add the database when configuring CotProject?",
        )
    }
}

/// Runs the Cot project on the given address.
///
/// This function takes a Cot project and an address string and runs the
/// project on the given address.
///
/// # Errors
///
/// This function returns an error if the server fails to start.
// Send not needed; Bootstrapper/CLI is run async in a single thread
#[allow(clippy::future_not_send)]
pub async fn run(bootstrapper: Bootstrapper<Initialized>, address_str: &str) -> cot::Result<()> {
    let listener = tokio::net::TcpListener::bind(address_str)
        .await
        .map_err(|e| ErrorRepr::StartServer { source: e })?;

    run_at(bootstrapper, listener).await
}

/// Runs the Cot project on the given listener.
///
/// This function takes a Cot project and a [`tokio::net::TcpListener`] and
/// runs the project on the given listener.
///
/// If you need more control over the server listening socket, such as modifying
/// the underlying buffer sizes, you can create a [`tokio::net::TcpListener`]
/// and pass it to this function. Otherwise, [`run`] function will be more
/// convenient.
///
/// # Errors
///
/// This function returns an error if the server fails to start.
// Send not needed; Bootstrapper/CLI is run async in a single thread
#[allow(clippy::future_not_send)]
pub async fn run_at(
    bootstrapper: Bootstrapper<Initialized>,
    listener: tokio::net::TcpListener,
) -> cot::Result<()> {
    let not_found_handler: Arc<dyn ErrorPageHandler> =
        bootstrapper.project().not_found_handler().into();
    let server_error_handler: Arc<dyn ErrorPageHandler> =
        bootstrapper.project().server_error_handler().into();
    let (mut context, mut project_handler) = bootstrapper.into_context_and_handler();

    #[cfg(feature = "db")]
    if let Some(database) = &context.database {
        let mut migrations: Vec<Box<SyncDynMigration>> = Vec::new();
        for app in &context.apps {
            migrations.extend(app.migrations());
        }
        let migration_engine = MigrationEngine::new(migrations)?;
        migration_engine.run(database).await?;
    }

    let mut apps = std::mem::take(&mut context.apps);
    for app in &mut apps {
        info!("Initializing app: {}", app.name());

        app.init(&mut context).await?;
    }
    context.apps = apps;

    let context = Arc::new(context);
    let is_debug = context.config().debug;
    let register_panic_hook = context.config().register_panic_hook;
    #[cfg(feature = "db")]
    let context_cleanup = context.clone();

    let handler = move |axum_request: axum::extract::Request| async move {
        let request = request_axum_to_cot(axum_request, Arc::clone(&context));
        let (request_parts, request) = request_parts_for_diagnostics(request);

        let catch_unwind_response = AssertUnwindSafe(pass_to_axum(request, &mut project_handler))
            .catch_unwind()
            .await;

        let response: Result<axum::response::Response, ErrorResponse> = match catch_unwind_response
        {
            Ok(response) => match response {
                Ok(response) => match response.extensions().get::<ErrorPageTrigger>() {
                    Some(trigger) => Err(ErrorResponse::ErrorPageTrigger(*trigger)),
                    None => Ok(response),
                },
                Err(error) => Err(ErrorResponse::ErrorReturned(error)),
            },
            Err(error) => Err(ErrorResponse::Panic(error)),
        };

        match response {
            Ok(response) => response,
            Err(error_response) => {
                if is_debug {
                    let diagnostics = Diagnostics::new(
                        context.config().clone(),
                        Arc::clone(&context.router),
                        request_parts,
                    );

                    build_cot_error_page(error_response, diagnostics)
                } else {
                    build_custom_error_page(
                        &not_found_handler,
                        &server_error_handler,
                        &error_response,
                    )
                }
            }
        }
    };

    eprintln!(
        "Starting the server at http://{}",
        listener
            .local_addr()
            .map_err(|e| ErrorRepr::StartServer { source: e })?
    );

    if register_panic_hook {
        let current_hook = std::panic::take_hook();
        let new_hook = move |hook_info: &std::panic::PanicHookInfo<'_>| {
            current_hook(hook_info);
            error_page::error_page_panic_hook(hook_info);
        };
        std::panic::set_hook(Box::new(new_hook));
    }
    axum::serve(listener, handler.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| ErrorRepr::StartServer { source: e })?;
    if register_panic_hook {
        let _ = std::panic::take_hook();
    }
    #[cfg(feature = "db")]
    if let Some(database) = &context_cleanup.database {
        database.close().await?;
    }

    Ok(())
}

enum ErrorResponse {
    ErrorPageTrigger(ErrorPageTrigger),
    ErrorReturned(Error),
    Panic(Box<dyn std::any::Any + Send>),
}

fn build_cot_error_page(
    error_response: ErrorResponse,
    diagnostics: Diagnostics,
) -> axum::response::Response {
    match error_response {
        ErrorResponse::ErrorPageTrigger(trigger) => match trigger {
            ErrorPageTrigger::NotFound => error_page::handle_not_found(diagnostics),
        },
        ErrorResponse::ErrorReturned(error) => {
            error_page::handle_response_error(error, diagnostics)
        }
        ErrorResponse::Panic(error) => error_page::handle_response_panic(error, diagnostics),
    }
}

fn build_custom_error_page(
    not_found_handler: &Arc<dyn ErrorPageHandler>,
    server_error_handler: &Arc<dyn ErrorPageHandler>,
    error_response: &ErrorResponse,
) -> axum::response::Response {
    match error_response {
        ErrorResponse::ErrorPageTrigger(ErrorPageTrigger::NotFound) => {
            not_found_handler.handle().map_or_else(
                |error| {
                    error!(
                        ?error,
                        "Error occurred while running custom 404 Not Found handler"
                    );
                    error_page::build_cot_not_found_page()
                },
                response_cot_to_axum,
            )
        }
        ErrorResponse::ErrorReturned(_) | ErrorResponse::Panic(_) => {
            server_error_handler.handle().map_or_else(
                |error| {
                    error!(
                        ?error,
                        "Error occurred while running custom 500 Internal Server Error handler"
                    );

                    error_page::build_cot_server_error_page()
                },
                response_cot_to_axum,
            )
        }
    }
}

/// Runs the CLI for the given project.
///
/// This function takes a [`Project`] and runs the CLI for the project. You
/// typically don't need to call this function directly. Instead, you can use
/// [`cot::main`] which is a more ergonomic way to run the CLI.
///
/// # Errors
///
/// This function returns an error if the CLI command fails to execute.
///
/// # Examples
///
/// ```no_run
/// use cot::{run_cli, App, Project};
///
/// struct MyProject;
/// impl Project for MyProject {}
///
/// # #[tokio::main]
/// # async fn main() -> cot::Result<()> {
/// run_cli(MyProject).await?;
/// # Ok(())
/// # }
/// ```
#[allow(clippy::future_not_send)] // Send not needed; CLI is run async in a single thread
pub async fn run_cli(project: impl Project + 'static) -> cot::Result<()> {
    Bootstrapper::new(project).run_cli().await
}

fn request_parts_for_diagnostics(request: Request) -> (Option<Parts>, Request) {
    if request.project_config().debug {
        let (parts, body) = request.into_parts();
        let parts_clone = parts.clone();
        let request = Request::from_parts(parts, body);
        (Some(parts_clone), request)
    } else {
        (None, request)
    }
}

fn request_axum_to_cot(
    axum_request: axum::extract::Request,
    context: Arc<ProjectContext>,
) -> Request {
    let mut request = axum_request.map(Body::axum);
    prepare_request(&mut request, context);
    request
}

pub(crate) fn prepare_request(request: &mut Request, context: Arc<ProjectContext>) {
    request.extensions_mut().insert(context);
}

async fn pass_to_axum(
    request: Request,
    handler: &mut BoxedHandler,
) -> cot::Result<axum::response::Response> {
    poll_fn(|cx| handler.poll_ready(cx)).await?;
    let response = handler.call(request).await?;

    Ok(response_cot_to_axum(response))
}

fn response_cot_to_axum(response: Response) -> axum::response::Response {
    response.map(axum::body::Body::new)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use cot::test::serial_guard;

    use super::*;
    use crate::auth::UserId;
    use crate::config::SecretKey;
    use crate::test::TestRequestBuilder;

    struct TestApp;

    impl App for TestApp {
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn app_default_impl() {
        let app = TestApp {};
        assert_eq!(app.name(), "mock");
        assert_eq!(app.router().routes().len(), 0);
        assert_eq!(app.migrations().len(), 0);
    }

    struct TestProject;
    impl Project for TestProject {}

    #[test]
    fn project_default_cli_metadata() {
        let metadata = TestProject.cli_metadata();

        assert_eq!(metadata.name, "cot");
        assert_eq!(metadata.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(metadata.authors, env!("CARGO_PKG_AUTHORS"));
        assert_eq!(metadata.description, env!("CARGO_PKG_DESCRIPTION"));
    }

    #[cfg(feature = "live-reload")]
    #[tokio::test]
    async fn project_middlewares() {
        struct TestProject;
        impl Project for TestProject {
            fn config(&self, config_name: &str) -> cot::Result<ProjectConfig> {
                Ok(ProjectConfig::default())
            }

            fn middlewares(
                &self,
                handler: RootHandlerBuilder,
                context: &ProjectContext<WithApps>,
            ) -> BoxedHandler {
                handler
                    .middleware(
                        crate::static_files::StaticFilesMiddleware::from_app_context(context),
                    )
                    .middleware(crate::middleware::LiveReloadMiddleware::from_app_context(
                        context,
                    ))
                    .build()
            }
        }

        let response = crate::test::Client::new(TestProject)
            .await
            .get("/")
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn project_default_config() {
        let temp_dir = tempfile::tempdir().unwrap();

        let config_dir = temp_dir.path().join("config");
        std::fs::create_dir(&config_dir).unwrap();
        let config = r#"
            debug = false
            secret_key = "123abc"
        "#;

        let config_file_path = config_dir.as_path().join("dev.toml");
        std::fs::write(config_file_path, config).unwrap();

        // ensure the tests run sequentially when setting the current directory
        let _guard = serial_guard();

        std::env::set_current_dir(&temp_dir).unwrap();
        let config = TestProject.config("dev").unwrap();

        assert!(!config.debug);
        assert_eq!(config.secret_key, SecretKey::from("123abc".to_string()));
    }

    #[test]
    fn project_default_register_apps() {
        let mut apps = AppBuilder::new();
        let context = ProjectContext::new().with_config(ProjectConfig::default());

        TestProject.register_apps(&mut apps, &context);

        assert!(apps.apps.is_empty());
    }

    #[tokio::test]
    async fn test_default_auth_backend() {
        let context = ProjectContext::new()
            .with_config(
                ProjectConfig::builder()
                    .auth_backend(AuthBackendConfig::None)
                    .build(),
            )
            .with_apps(vec![], Arc::new(Router::empty()));

        let auth_backend = TestProject.auth_backend(&context);
        assert!(auth_backend
            .get_by_id(&TestRequestBuilder::get("/").build(), UserId::Int(0))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
    async fn bootstrapper() {
        struct TestProject;
        impl Project for TestProject {
            fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {
                apps.register_with_views(TestApp {}, "/app");
            }
        }

        let bootstrapper = Bootstrapper::new(TestProject)
            .with_config(ProjectConfig::default())
            .boot()
            .await
            .unwrap();

        assert_eq!(bootstrapper.context().apps.len(), 1);
        assert_eq!(bootstrapper.context().router.routes().len(), 1);
    }
}
