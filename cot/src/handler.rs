use std::future::Future;

use async_trait::async_trait;
use tower::util::BoxCloneService;

use crate::request::Request;
use crate::response::Response;
use crate::{Error, Result};

/// A function that takes a request and returns a response.
///
/// This is the main building block of a Cot app. You shouldn't
/// usually need to implement this directly, as it is already
/// implemented for closures and functions that take a [`Request`]
/// and return a [`Result<Response>`].
#[async_trait]
pub trait RequestHandler {
    /// Handle the request and returns a response.
    ///
    /// # Errors
    ///
    /// This method can return an error if the request handler fails to handle
    /// the request.
    async fn handle(&self, request: Request) -> Result<Response>;
}

#[async_trait]
impl<T, R> RequestHandler for T
where
    T: Fn(Request) -> R + Clone + Send + Sync + 'static,
    R: for<'a> Future<Output = Result<Response>> + Send,
{
    async fn handle(&self, request: Request) -> Result<Response> {
        self(request).await
    }
}

/// A wrapper around a handler that's used in
/// [`Bootstrapper`](cot::Bootstrapper).
///
/// It is returned by
/// [`Bootstrapper::into_context_and_handler`](cot::Bootstrapper::into_context_and_handler).
/// Typically, you don't need to interact with this type directly, except for
/// creating it in [`Project::middlewares`](cot::Project::middlewares) through
/// the [`RootHandlerBuilder::build`](cot::project::RootHandlerBuilder::build).
/// method.
///
/// # Examples
///
/// ```
/// use cot::config::ProjectConfig;
/// use cot::project::{RootHandlerBuilder, WithApps};
/// use cot::static_files::StaticFilesMiddleware;
/// use cot::{Bootstrapper, BoxedHandler, Project, ProjectContext};
///
/// struct MyProject;
/// impl Project for MyProject {
///     fn middlewares(
///         &self,
///         handler: RootHandlerBuilder,
///         context: &ProjectContext<WithApps>,
///     ) -> BoxedHandler {
///         handler
///             .middleware(StaticFilesMiddleware::from_app_context(context))
///             .build()
///     }
/// }
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
pub type BoxedHandler = BoxCloneService<Request, Response, Error>;
