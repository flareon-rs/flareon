use std::future::poll_fn;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use async_trait::async_trait;
use axum::handler::HandlerWithoutStateExt;
use bytes::Bytes;
use derive_more::Debug;
use futures_util::FutureExt;
use http::request::Parts;
use tower::{Layer, Service};
use tracing::info;

use crate::admin::AdminModelManager;
use crate::cli::Cli;
#[cfg(feature = "db")]
use crate::config::DatabaseConfig;
use crate::config::ProjectConfig;
#[cfg(feature = "db")]
use crate::db::migrations::{MigrationEngine, SyncDynMigration};
#[cfg(feature = "db")]
use crate::db::Database;
use crate::error::ErrorRepr;
use crate::error_page::{CotDiagnostics, ErrorPageTrigger};
use crate::handler::BoxedHandler;
use crate::middleware::{IntoCotError, IntoCotErrorLayer, IntoCotResponse, IntoCotResponseLayer};
use crate::request::Request;
use crate::response::Response;
use crate::router::{Route, Router, RouterService};
use crate::{cli, config, error_page, Body, Error};

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
pub trait CotApp: Send + Sync {
    /// The name of the app.
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
    async fn init(&self, context: &mut AppContext) -> cot::Result<()> {
        Ok(())
    }

    /// Returns the router for the app. By default, it returns an empty router.
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

/// A Cot project, ready to be run.
#[derive(Debug)]
pub struct CotProject {
    context: AppContext,
    handler: BoxedHandler,
    cli: Cli,
}

/// A part of [`CotProject`] that contains the shared context and configs
/// for all apps.
#[derive(Debug)]
pub struct AppContext {
    config: Arc<ProjectConfig>,
    #[debug("...")]
    apps: Vec<Box<dyn CotApp>>,
    router: Arc<Router>,
    #[cfg(feature = "db")]
    database: Option<Arc<Database>>,
}

impl AppContext {
    #[must_use]
    pub(crate) fn new(
        config: Arc<ProjectConfig>,
        apps: Vec<Box<dyn CotApp>>,
        router: Arc<Router>,
        #[cfg(feature = "db")] database: Option<Arc<Database>>,
    ) -> Self {
        Self {
            config,
            apps,
            router,
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
    /// use cot::CotProject;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let config = request.context().config();
    ///     // can also be accessed via:
    ///     let config = request.project_config();
    ///
    ///     let db_url = config.database_config().url();
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
    /// use cot::CotProject;
    ///
    /// async fn index(request: Request) -> cot::Result<Response> {
    ///     let apps = request.context().apps();
    ///
    ///     // ...
    /// #    todo!()
    /// }
    /// ```
    #[must_use]
    pub fn apps(&self) -> &[Box<dyn CotApp>] {
        &self.apps
    }

    /// Returns the router for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    /// use cot::CotProject;
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

    /// Returns the database for the project, if it is enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    /// use cot::CotProject;
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
    /// use cot::CotProject;
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

#[doc(hidden)]
#[derive(Debug, Copy, Clone)]
pub struct Uninitialized;

/// The builder for the [`CotProject`].
#[derive(Debug)]
pub struct CotProjectBuilder<S> {
    context: AppContext,
    cli: Cli,
    urls: Vec<Route>,
    handler: S,
}

impl CotProjectBuilder<Uninitialized> {
    #[must_use]
    fn new() -> Self {
        Self {
            context: AppContext {
                config: Arc::new(ProjectConfig::default()),
                apps: vec![],
                router: Arc::new(Router::default()),
                #[cfg(feature = "db")]
                database: None,
            },
            cli: Cli::new(),
            urls: Vec::new(),
            handler: Uninitialized,
        }
    }

    /// Sets the metadata for the CLI.
    ///
    /// This method is used to set the name, version, authors, and description
    /// of the CLI application. This is meant to be typically used with
    /// [`crate::cli::metadata!`].
    #[must_use]
    pub fn with_cli(mut self, metadata: cli::CliMetadata) -> Self {
        self.cli.set_metadata(metadata);
        self
    }

    /// Adds a task to the CLI.
    ///
    /// This method is used to add a task to the CLI. The task will be available
    /// as a subcommand of the main CLI command.
    #[must_use]
    pub fn add_task<C>(mut self, task: C) -> Self
    where
        C: cli::CliTask + Send + 'static,
    {
        self.cli.add_task(task);
        self
    }

    /// Sets the configuration for the project.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    /// use cot::CotProject;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let cot_project = CotProject::builder()
    ///     .config(ProjectConfig::default())
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn config(mut self, config: ProjectConfig) -> Self {
        self.context.config = Arc::new(config);
        self
    }

    /// Registers an app with views.
    ///
    /// This method is used to register an app with views. The app's views will
    /// be available at the given URL prefix.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cot::{CotApp, CotProject};
    ///
    /// struct HelloApp;
    ///
    /// impl CotApp for HelloApp {
    ///     fn name(&self) -> &'static str {
    ///         env!("CARGO_PKG_NAME")
    ///     }
    /// }
    ///
    /// #[cot::main]
    /// async fn main() -> cot::Result<CotProject> {
    ///     let cot_project = CotProject::builder()
    ///         .register_app_with_views(HelloApp, "/hello")
    ///         .build()
    ///         .await?;
    ///
    ///     Ok(cot_project)
    /// }
    /// ```
    #[must_use]
    pub fn register_app_with_views<T: CotApp + 'static>(
        mut self,
        app: T,
        url_prefix: &str,
    ) -> Self {
        self.urls.push(Route::with_router(url_prefix, app.router()));
        self = self.register_app(app);
        self
    }

    /// Registers an app.
    ///
    /// This method is used to register an app. The app's views, if any, will
    /// not be available.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cot::{CotApp, CotProject};
    ///
    /// struct HelloApp;
    ///
    /// impl CotApp for HelloApp {
    ///     fn name(&self) -> &'static str {
    ///         env!("CARGO_PKG_NAME")
    ///     }
    /// }
    ///
    /// #[cot::main]
    /// async fn main() -> cot::Result<CotProject> {
    ///     let cot_project = CotProject::builder().register_app(HelloApp).build().await?;
    ///
    ///     Ok(cot_project)
    /// }
    /// ```
    #[must_use]
    pub fn register_app<T: CotApp + 'static>(mut self, app: T) -> Self {
        self.context.apps.push(Box::new(app));
        self
    }

    /// Adds middleware to the project.
    ///
    /// This method is used to add middleware to the project. The middleware
    /// will be applied to all routes in the project.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cot::middleware::LiveReloadMiddleware;
    /// use cot::{CotApp, CotProject};
    ///
    /// #[cot::main]
    /// async fn main() -> cot::Result<CotProject> {
    ///     let cot_project = CotProject::builder()
    ///         .middleware(LiveReloadMiddleware::new())
    ///         .build()
    ///         .await?;
    ///
    ///     Ok(cot_project)
    /// }
    /// ```
    #[must_use]
    pub fn middleware<M>(
        self,
        middleware: M,
    ) -> CotProjectBuilder<IntoCotError<IntoCotResponse<M::Service>>>
    where
        M: Layer<RouterService>,
    {
        self.into_builder_with_service().middleware(middleware)
    }

    /// Adds middleware to the project, with access to the project context.
    ///
    /// The project context might be useful for creating middlewares that need
    /// access to the project's configuration, apps, database, etc. An example
    /// of such middleware is the [`StaticFilesMiddleware`], which iterates
    /// through all the registered apps and collects the static files from them.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::static_files::StaticFilesMiddleware;
    /// use cot::{CotApp, CotProject};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let cot_project = CotProject::builder()
    ///     .middleware_with_context(StaticFilesMiddleware::from_app_context)
    ///     .build()
    ///     .await?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn middleware_with_context<M, F>(
        self,
        get_middleware: F,
    ) -> CotProjectBuilder<IntoCotError<IntoCotResponse<M::Service>>>
    where
        M: Layer<RouterService>,
        F: FnOnce(&AppContext) -> M,
    {
        self.into_builder_with_service()
            .middleware_with_context(get_middleware)
    }

    /// Builds the Cot project instance.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cot::{CotApp, CotProject};
    ///
    /// #[cot::main]
    /// async fn main() -> cot::Result<CotProject> {
    ///     let cot_project = CotProject::builder().build().await?;
    ///
    ///     Ok(cot_project)
    /// }
    /// ```
    pub async fn build(self) -> cot::Result<CotProject> {
        self.into_builder_with_service().build().await
    }

    #[must_use]
    fn into_builder_with_service(mut self) -> CotProjectBuilder<RouterService> {
        let router = Arc::new(Router::with_urls(self.urls));
        self.context.router = Arc::clone(&router);

        CotProjectBuilder {
            context: self.context,
            cli: self.cli,
            urls: vec![],
            handler: RouterService::new(router),
        }
    }
}

impl<S> CotProjectBuilder<S>
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
    /// ```no_run
    /// use cot::middleware::LiveReloadMiddleware;
    /// use cot::{CotApp, CotProject};
    ///
    /// struct HelloApp;
    ///
    /// impl CotApp for HelloApp {
    ///     fn name(&self) -> &'static str {
    ///         env!("CARGO_PKG_NAME")
    ///     }
    /// }
    ///
    /// #[cot::main]
    /// async fn main() -> cot::Result<CotProject> {
    ///     let cot_project = CotProject::builder()
    ///         .register_app(HelloApp)
    ///         .middleware(LiveReloadMiddleware::new())
    ///         .build()
    ///         .await?;
    ///
    ///     Ok(cot_project)
    /// }
    /// ```
    #[must_use]
    pub fn middleware<M>(
        self,
        middleware: M,
    ) -> CotProjectBuilder<IntoCotError<IntoCotResponse<<M as Layer<S>>::Service>>>
    where
        M: Layer<S>,
    {
        let layer = (
            IntoCotErrorLayer::new(),
            IntoCotResponseLayer::new(),
            middleware,
        );

        CotProjectBuilder {
            context: self.context,
            cli: self.cli,
            urls: vec![],
            handler: layer.layer(self.handler),
        }
    }

    /// Adds middleware to the project, with access to the project context.
    ///
    /// The project context might be useful for creating middlewares that need
    /// access to the project's configuration, apps, database, etc. An example
    /// of such middleware is the [`StaticFilesMiddleware`], which iterates
    /// through all the registered apps and collects the static files from them.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::static_files::StaticFilesMiddleware;
    /// use cot::{CotApp, CotProject};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let cot_project = CotProject::builder()
    ///     .middleware_with_context(StaticFilesMiddleware::from_app_context)
    ///     .build()
    ///     .await?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn middleware_with_context<M, F>(
        self,
        get_middleware: F,
    ) -> CotProjectBuilder<IntoCotError<IntoCotResponse<<M as Layer<S>>::Service>>>
    where
        M: Layer<S>,
        F: FnOnce(&AppContext) -> M,
    {
        let middleware = get_middleware(&self.context);
        self.middleware(middleware)
    }

    /// Builds the Cot project instance.
    pub async fn build(mut self) -> cot::Result<CotProject> {
        #[cfg(feature = "db")]
        {
            let database = Self::init_database(self.context.config.database_config()).await?;
            self.context.database = Some(database);
        }

        Ok(CotProject {
            context: self.context,
            cli: self.cli,
            handler: BoxedHandler::new(self.handler),
        })
    }

    #[cfg(feature = "db")]
    async fn init_database(config: &DatabaseConfig) -> cot::Result<Arc<Database>> {
        let database = Database::new(config.url()).await?;
        Ok(Arc::new(database))
    }
}

impl Default for CotProjectBuilder<Uninitialized> {
    fn default() -> Self {
        Self::new()
    }
}

impl CotProject {
    /// Creates a new builder for the [`CotProject`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cot::{CotApp, CotProject};
    ///
    /// struct HelloApp;
    ///
    /// impl CotApp for HelloApp {
    ///     fn name(&self) -> &'static str {
    ///         env!("CARGO_PKG_NAME")
    ///     }
    /// }
    ///
    /// #[cot::main]
    /// async fn main() -> cot::Result<CotProject> {
    ///     let cot_project = CotProject::builder().build().await?;
    ///
    ///     Ok(cot_project)
    /// }
    /// ```
    #[must_use]
    pub fn builder() -> CotProjectBuilder<Uninitialized> {
        CotProjectBuilder::default()
    }

    /// Returns the context of the project.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cot::CotProject;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let project = CotProject::builder().build().await?;
    /// let context = project.context();
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn context(&self) -> &AppContext {
        &self.context
    }

    /// Returns the context and handler of the project.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cot::CotProject;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// let (context, handler) = CotProject::builder().build().await?.into_context();
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn into_context(self) -> (AppContext, BoxedHandler) {
        (self.context, self.handler)
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
pub async fn run(project: CotProject, address_str: &str) -> cot::Result<()> {
    let listener = tokio::net::TcpListener::bind(address_str)
        .await
        .map_err(|e| ErrorRepr::StartServer { source: e })?;

    run_at(project, listener).await
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
pub async fn run_at(project: CotProject, listener: tokio::net::TcpListener) -> cot::Result<()> {
    let (mut context, mut project_handler) = project.into_context();

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
    #[cfg(feature = "db")]
    let context_cleanup = context.clone();

    let handler = |axum_request: axum::extract::Request| async move {
        let request = request_axum_to_cot(axum_request, Arc::clone(&context));
        let (request_parts, request) = request_parts_for_diagnostics(request);

        let catch_unwind_response = AssertUnwindSafe(pass_to_axum(request, &mut project_handler))
            .catch_unwind()
            .await;

        let show_error_page = match &catch_unwind_response {
            Ok(response) => match response {
                Ok(response) => response.extensions().get::<ErrorPageTrigger>().is_some(),
                Err(_) => true,
            },
            Err(_) => {
                // handler panicked
                true
            }
        };

        if show_error_page {
            let diagnostics = CotDiagnostics::new(Arc::clone(&context.router), request_parts);

            match catch_unwind_response {
                Ok(response) => match response {
                    Ok(response) => match response
                        .extensions()
                        .get::<ErrorPageTrigger>()
                        .expect("ErrorPageTrigger already has been checked to be Some")
                    {
                        ErrorPageTrigger::NotFound => error_page::handle_not_found(diagnostics),
                    },
                    Err(error) => error_page::handle_response_error(error, diagnostics),
                },
                Err(error) => error_page::handle_response_panic(error, diagnostics),
            }
        } else {
            catch_unwind_response
                .expect("Error page should be shown if the response is not a panic")
                .expect("Error page should be shown if the response is not an error")
        }
    };

    eprintln!(
        "Starting the server at http://{}",
        listener
            .local_addr()
            .map_err(|e| ErrorRepr::StartServer { source: e })?
    );
    if config::REGISTER_PANIC_HOOK {
        let current_hook = std::panic::take_hook();
        let new_hook = move |hook_info: &std::panic::PanicHookInfo<'_>| {
            current_hook(hook_info);
            error_page::error_page_panic_hook(hook_info);
        };
        std::panic::set_hook(Box::new(new_hook));
    }
    axum::serve(listener, handler.into_make_service())
        .await
        .map_err(|e| ErrorRepr::StartServer { source: e })?;
    if config::REGISTER_PANIC_HOOK {
        let _ = std::panic::take_hook();
    }
    #[cfg(feature = "db")]
    if let Some(database) = &context_cleanup.database {
        database.close().await?;
    }

    Ok(())
}

/// Runs the CLI for the given project.
///
/// This function takes a [`CotProject`] and runs the CLI for the project. You
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
/// use cot::{run_cli, CotProject};
///
/// # #[tokio::main]
/// # async fn main() -> cot::Result<()> {
/// let project = CotProject::builder().build().await?;
/// run_cli(project).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run_cli(mut project: CotProject) -> cot::Result<()> {
    std::mem::take(&mut project.cli).execute(project).await
}

fn request_parts_for_diagnostics(request: Request) -> (Option<Parts>, Request) {
    if config::DEBUG_MODE {
        let (parts, body) = request.into_parts();
        let parts_clone = parts.clone();
        let request = Request::from_parts(parts, body);
        (Some(parts_clone), request)
    } else {
        (None, request)
    }
}

fn request_axum_to_cot(axum_request: axum::extract::Request, context: Arc<AppContext>) -> Request {
    let mut request = axum_request.map(Body::axum);
    prepare_request(&mut request, context);
    request
}

pub(crate) fn prepare_request(request: &mut Request, context: Arc<AppContext>) {
    request.extensions_mut().insert(context);
}

async fn pass_to_axum(
    request: Request,
    handler: &mut BoxedHandler,
) -> cot::Result<axum::response::Response> {
    poll_fn(|cx| handler.poll_ready(cx)).await?;
    let response = handler.call(request).await?;

    Ok(response.map(axum::body::Body::new))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCotApp;

    impl CotApp for MockCotApp {
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn cot_app_default_impl() {
        let app = MockCotApp {};
        assert_eq!(app.name(), "mock");
        assert_eq!(app.router().routes().len(), 0);
        assert_eq!(app.migrations().len(), 0);
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
    async fn cot_project_builder() {
        let project = CotProject::builder()
            .register_app_with_views(MockCotApp {}, "/app")
            .build()
            .await
            .unwrap();
        assert_eq!(project.context().apps.len(), 1);
        assert!(!project.context().router.is_empty());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
    async fn cot_project_router() {
        let project = CotProject::builder()
            .register_app_with_views(MockCotApp {}, "/app")
            .build()
            .await
            .unwrap();
        assert_eq!(project.context.router.routes().len(), 1);
    }
}
