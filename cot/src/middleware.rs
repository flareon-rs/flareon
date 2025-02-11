//! Middlewares for modifying requests and responses.
//!
//! Middlewares are used to modify requests and responses in a pipeline. They
//! are used to add functionality to the request/response cycle, such as
//! session management, adding security headers, and more.

use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::TryFutureExt;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use tower::Service;
use tower_sessions::{MemoryStore, SessionManagerLayer};

use crate::error::ErrorRepr;
use crate::request::Request;
use crate::response::Response;
use crate::{Body, Error};

/// Middleware that converts a any [`http::Response`] generic type to a
/// [`cot::Response`].
///
/// This is useful for converting a response from a middleware that is
/// compatible with the `tower` crate to a response that is compatible with
/// Cot. It's applied automatically by
/// [`RootHandlerBuilder::middleware()`](cot::project::RootHandlerBuilder::middleware())
/// and is not needed to be added manually.
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
///             // IntoCotResponseLayer used internally in middleware()
///             .middleware(LiveReloadMiddleware::from_app_context(context))
///             .build()
///     }
/// }
/// ```
#[derive(Debug, Copy, Clone)]
pub struct IntoCotResponseLayer;

impl IntoCotResponseLayer {
    /// Create a new [`IntoCotResponseLayer`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::middleware::IntoCotResponseLayer;
    ///
    /// let middleware = IntoCotResponseLayer::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for IntoCotResponseLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> tower::Layer<S> for IntoCotResponseLayer {
    type Service = IntoCotResponse<S>;

    fn layer(&self, inner: S) -> Self::Service {
        IntoCotResponse { inner }
    }
}

/// Service struct that converts a any [`http::Response`] generic type to a
/// [`cot::Response`].
///
/// Used by [`IntoCotResponseLayer`].
///
/// # Examples
///
/// ```
/// use std::any::TypeId;
///
/// use cot::middleware::{IntoCotResponse, IntoCotResponseLayer};
///
/// assert_eq!(
///     TypeId::of::<<IntoCotResponseLayer as tower::Layer<()>>::Service>(),
///     TypeId::of::<IntoCotResponse::<()>>()
/// );
/// ```
#[derive(Debug, Clone)]
pub struct IntoCotResponse<S> {
    inner: S,
}

impl<S, B, E> Service<Request> for IntoCotResponse<S>
where
    S: Service<Request, Response = http::Response<B>>,
    B: http_body::Body<Data = Bytes, Error = E> + Send + Sync + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = futures_util::future::MapOk<S::Future, fn(http::Response<B>) -> Response>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[inline]
    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map_ok(map_response)
    }
}

fn map_response<B, E>(response: http::response::Response<B>) -> Response
where
    B: http_body::Body<Data = Bytes, Error = E> + Send + Sync + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    response.map(|body| Body::wrapper(BoxBody::new(body.map_err(map_err))))
}

/// Middleware that converts a any error type to a
/// [`cot::Error`].
///
/// This is useful for converting a response from a middleware that is
/// compatible with the `tower` crate to a response that is compatible with
/// Cot. It's applied automatically by
/// [`RootHandlerBuilder::middleware()`](cot::project::RootHandlerBuilder::middleware())
/// and is not needed to be added manually.
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
///             // IntoCotErrorLayer used internally in middleware()
///             .middleware(LiveReloadMiddleware::from_app_context(context))
///             .build()
///     }
/// }
/// ```
#[derive(Debug, Copy, Clone)]
pub struct IntoCotErrorLayer;

impl IntoCotErrorLayer {
    /// Create a new [`IntoCotErrorLayer`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::middleware::IntoCotErrorLayer;
    ///
    /// let middleware = IntoCotErrorLayer::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for IntoCotErrorLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> tower::Layer<S> for IntoCotErrorLayer {
    type Service = IntoCotError<S>;

    fn layer(&self, inner: S) -> Self::Service {
        IntoCotError { inner }
    }
}

/// Service struct that converts a any error type to a [`cot::Error`].
///
/// Used by [`IntoCotErrorLayer`].
///
/// # Examples
///
/// ```
/// use std::any::TypeId;
///
/// use cot::middleware::{IntoCotError, IntoCotErrorLayer};
///
/// assert_eq!(
///     TypeId::of::<<IntoCotErrorLayer as tower::Layer<()>>::Service>(),
///     TypeId::of::<IntoCotError::<()>>()
/// );
/// ```
#[derive(Debug, Clone)]
pub struct IntoCotError<S> {
    inner: S,
}

impl<S> Service<Request> for IntoCotError<S>
where
    S: Service<Request>,
    <S as Service<Request>>::Error: std::error::Error + Send + Sync + 'static,
{
    type Response = S::Response;
    type Error = Error;
    type Future = futures_util::future::MapErr<S::Future, fn(S::Error) -> Error>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(map_err)
    }

    #[inline]
    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map_err(map_err)
    }
}

fn map_err<E>(error: E) -> Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    Error::new(ErrorRepr::MiddlewareWrapped {
        source: Box::new(error),
    })
}

#[derive(Debug, Copy, Clone)]
pub struct SessionMiddleware;

impl SessionMiddleware {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for SessionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> tower::Layer<S> for SessionMiddleware {
    type Service = <SessionManagerLayer<MemoryStore> as tower::Layer<S>>::Service;

    fn layer(&self, inner: S) -> Self::Service {
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store);
        session_layer.layer(inner)
    }
}

#[cfg(feature = "live-reload")]
type LiveReloadLayerType = tower::util::Either<
    (
        IntoCotErrorLayer,
        IntoCotResponseLayer,
        tower_livereload::LiveReloadLayer,
    ),
    tower::layer::util::Identity,
>;

#[cfg(feature = "live-reload")]
#[derive(Debug, Clone)]
pub struct LiveReloadMiddleware(LiveReloadLayerType);

#[cfg(feature = "live-reload")]
impl LiveReloadMiddleware {
    #[must_use]
    pub fn new() -> Self {
        Self::with_enabled(true)
    }

    #[must_use]
    pub fn from_app_context(app_context: &crate::ProjectContext<crate::project::WithApps>) -> Self {
        Self::with_enabled(app_context.config().middlewares.live_reload.enabled)
    }

    fn with_enabled(enabled: bool) -> Self {
        let option_layer = enabled.then(|| {
            (
                IntoCotErrorLayer::new(),
                IntoCotResponseLayer::new(),
                tower_livereload::LiveReloadLayer::new(),
            )
        });
        Self(tower::util::option_layer(option_layer))
    }
}

#[cfg(feature = "live-reload")]
impl Default for LiveReloadMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "live-reload")]
impl<S> tower::Layer<S> for LiveReloadMiddleware {
    type Service = <LiveReloadLayerType as tower::Layer<S>>::Service;

    fn layer(&self, inner: S) -> Self::Service {
        self.0.layer(inner)
    }
}

// TODO: add Cot ORM-based session store
