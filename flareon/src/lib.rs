#![warn(
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate self as flareon;

pub mod db;
mod error;
pub mod forms;
mod headers;
// Not public API. Referenced by macro-generated code.
#[doc(hidden)]
#[path = "private.rs"]
pub mod __private;
pub mod auth;
pub mod config;
mod error_page;
pub mod middleware;
pub mod request;
pub mod response;
pub mod router;
pub mod test;

use std::fmt::{Debug, Formatter};
use std::future::{poll_fn, Future};
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use axum::handler::HandlerWithoutStateExt;
use bytes::Bytes;
use derive_builder::Builder;
use derive_more::{Deref, From};
pub use error::Error;
use flareon::router::RouterService;
use futures_core::Stream;
use futures_util::FutureExt;
use http::request::Parts;
use http_body::{Frame, SizeHint};
use log::info;
use request::Request;
use router::{Route, Router};
use sync_wrapper::SyncWrapper;
use tower::Service;

use crate::config::ProjectConfig;
use crate::error::ErrorRepr;
use crate::error_page::{ErrorPageTrigger, FlareonDiagnostics};
use crate::response::Response;

/// A type alias for a result that can return a `flareon::Error`.
pub type Result<T> = std::result::Result<T, Error>;

/// A type alias for an HTTP status code.
pub type StatusCode = http::StatusCode;

/// A type alias for an HTTP method.
pub type Method = http::Method;

#[async_trait]
pub trait RequestHandler {
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

/// A building block for a Flareon project.
///
/// A Flareon app is a part (ideally, reusable) of a Flareon project that is
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
#[derive(Clone, Debug, Builder)]
#[builder(setter(into))]
pub struct FlareonApp {
    router: Router,
}

impl FlareonApp {
    #[must_use]
    pub fn builder() -> FlareonAppBuilder {
        FlareonAppBuilder::default()
    }
}

impl FlareonAppBuilder {
    #[allow(unused_mut)]
    pub fn urls<T: Into<Vec<Route>>>(&mut self, urls: T) -> &mut Self {
        // TODO throw error if urls have already been set
        self.router = Some(Router::with_urls(urls.into()));
        self
    }
}

/// A type that represents an HTTP request or response body.
///
/// This type is used to represent the body of an HTTP request/response. It can
/// be either a fixed body (e.g., a string or a byte array) or a streaming body
/// (e.g., a large file or a database query result).
///
/// # Examples
///
/// ```
/// use flareon::Body;
///
/// let body = Body::fixed("Hello, world!");
/// let body = Body::streaming(futures::stream::once(async { Ok("Hello, world!".into()) }));
/// ```
#[derive(Debug)]
pub struct Body {
    inner: BodyInner,
}

enum BodyInner {
    Fixed(Bytes),
    Streaming(SyncWrapper<Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>>),
    Axum(SyncWrapper<axum::body::Body>),
}

impl Debug for BodyInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(data) => f.debug_tuple("Fixed").field(data).finish(),
            Self::Streaming(_) => f.debug_tuple("Streaming").field(&"...").finish(),
            Self::Axum(axum_body) => f.debug_tuple("Axum").field(axum_body).finish(),
        }
    }
}

impl Body {
    #[must_use]
    const fn new(inner: BodyInner) -> Self {
        Self { inner }
    }

    /// Create an empty body.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::Body;
    ///
    /// let body = Body::empty();
    /// ```
    #[must_use]
    pub const fn empty() -> Self {
        Self::new(BodyInner::Fixed(Bytes::new()))
    }

    /// Create a body instance with the given fixed data.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::Body;
    ///
    /// let body = Body::fixed("Hello, world!");
    /// ```
    #[must_use]
    pub fn fixed<T: Into<Bytes>>(data: T) -> Self {
        Self::new(BodyInner::Fixed(data.into()))
    }

    /// Create a body instance from a stream of data.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_stream::stream;
    /// use flareon::Body;
    ///
    /// let stream = stream! {
    ///    yield Ok("Hello, ".into());
    ///    yield Ok("world!".into());
    /// };
    /// let body = Body::streaming(stream);
    /// ```
    #[must_use]
    pub fn streaming<T: Stream<Item = Result<Bytes>> + Send + 'static>(stream: T) -> Self {
        Self::new(BodyInner::Streaming(SyncWrapper::new(Box::pin(stream))))
    }

    pub async fn into_bytes(self) -> std::result::Result<Bytes, Error> {
        self.into_bytes_limited(usize::MAX).await
    }

    pub async fn into_bytes_limited(self, limit: usize) -> std::result::Result<Bytes, Error> {
        use http_body_util::BodyExt;

        Ok(http_body_util::Limited::new(self, limit)
            .collect()
            .await
            .map(http_body_util::Collected::to_bytes)
            .map_err(|source| ErrorRepr::ReadRequestBody { source })?)
    }

    #[must_use]
    fn axum(inner: axum::body::Body) -> Self {
        Self::new(BodyInner::Axum(SyncWrapper::new(inner)))
    }
}

impl Default for Body {
    fn default() -> Self {
        Self::empty()
    }
}

impl http_body::Body for Body {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut().inner {
            BodyInner::Fixed(ref mut data) => {
                if data.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = std::mem::take(data);
                    Poll::Ready(Some(Ok(Frame::data(data))))
                }
            }
            BodyInner::Streaming(ref mut stream) => {
                let stream = Pin::as_mut(stream.get_mut());
                match stream.poll_next(cx) {
                    Poll::Ready(Some(result)) => Poll::Ready(Some(result.map(Frame::data))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
            BodyInner::Axum(ref mut axum_body) => {
                let axum_body = axum_body.get_mut();
                Pin::new(axum_body).poll_frame(cx).map_err(|error| {
                    ErrorRepr::ReadRequestBody {
                        source: Box::new(error),
                    }
                    .into()
                })
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match &self.inner {
            BodyInner::Fixed(data) => data.is_empty(),
            BodyInner::Streaming(_) => false,
            BodyInner::Axum(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match &self.inner {
            BodyInner::Fixed(data) => SizeHint::with_exact(data.len() as u64),
            BodyInner::Streaming(_) => SizeHint::new(),
            BodyInner::Axum(_) => SizeHint::new(),
        }
    }
}

#[derive(Clone, Debug)]
// TODO add Middleware type?
pub struct FlareonProject<S> {
    config: Arc<ProjectConfig>,
    apps: Vec<FlareonApp>,
    router: Arc<Router>,
    handler: S,
}

#[derive(Debug, Clone)]
pub struct FlareonProjectBuilder {
    config: ProjectConfig,
    apps: Vec<FlareonApp>,
    urls: Vec<Route>,
}

impl FlareonProjectBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: ProjectConfig::default(),
            apps: Vec::new(),
            urls: Vec::new(),
        }
    }

    pub fn config(&mut self, config: ProjectConfig) -> &mut Self {
        self.config = config;
        self
    }

    pub fn register_app_with_views(&mut self, app: FlareonApp, url_prefix: &str) -> &mut Self {
        let new = self;
        new.urls
            .push(Route::with_router(url_prefix, app.router.clone()));
        new.apps.push(app);
        new
    }

    #[must_use]
    pub fn middleware<M: tower::Layer<RouterService>>(
        &mut self,
        middleware: M,
    ) -> FlareonProjectBuilderWithMiddleware<M::Service> {
        self.clone()
            .to_builder_with_middleware()
            .middleware(middleware)
    }

    /// Builds the Flareon project instance.
    #[must_use]
    pub fn build(&self) -> FlareonProject<RouterService> {
        self.to_builder_with_middleware().build()
    }

    #[must_use]
    fn to_builder_with_middleware(&self) -> FlareonProjectBuilderWithMiddleware<RouterService> {
        let config = Arc::new(self.config.clone());
        let router = Arc::new(Router::with_urls(self.urls.clone()));
        let service = RouterService::new(router.clone());

        FlareonProjectBuilderWithMiddleware::new(config, self.apps.clone(), router, service)
    }
}

#[derive(Debug)]
pub struct FlareonProjectBuilderWithMiddleware<S> {
    config: Arc<ProjectConfig>,
    apps: Vec<FlareonApp>,
    router: Arc<Router>,
    handler: S,
}

impl<S: Service<Request>> FlareonProjectBuilderWithMiddleware<S> {
    #[must_use]
    fn new(
        config: Arc<ProjectConfig>,
        apps: Vec<FlareonApp>,
        router: Arc<Router>,
        handler: S,
    ) -> Self {
        Self {
            config,
            apps,
            router,
            handler,
        }
    }

    #[must_use]
    pub fn middleware<M: tower::Layer<S>>(
        self,
        middleware: M,
    ) -> FlareonProjectBuilderWithMiddleware<M::Service> {
        FlareonProjectBuilderWithMiddleware {
            config: self.config,
            apps: self.apps,
            router: self.router,
            handler: middleware.layer(self.handler),
        }
    }

    /// Builds the Flareon project instance.
    #[must_use]
    pub fn build(self) -> FlareonProject<S> {
        FlareonProject {
            config: self.config,
            apps: self.apps,
            router: self.router,
            handler: self.handler,
        }
    }
}

impl Default for FlareonProjectBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FlareonProject<()> {
    #[must_use]
    pub fn builder() -> FlareonProjectBuilder {
        FlareonProjectBuilder::default()
    }
}

impl<S> FlareonProject<S>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
{
    #[must_use]
    pub fn router(&self) -> &Router {
        &self.router
    }
}

/// Runs the Flareon project.
///
/// This function takes a Flareon project and an address string and runs the
/// project on the given address.
///
/// # Errors
///
/// This function returns an error if the server fails to start.
pub async fn run<S>(project: FlareonProject<S>, address_str: &str) -> Result<()>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
    S::Future: Send,
{
    let listener = tokio::net::TcpListener::bind(address_str)
        .await
        .map_err(|e| ErrorRepr::StartServer { source: e })?;

    run_at(project, listener).await
}

/// Runs the Flareon project.
///
/// This function takes a Flareon project and a [`tokio::net::TcpListener`] and
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
pub async fn run_at<S>(
    mut project: FlareonProject<S>,
    listener: tokio::net::TcpListener,
) -> Result<()>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
    S::Future: Send,
{
    for app in &mut project.apps {
        info!("Initializing app: {:?}", app);
    }

    let FlareonProject {
        config,
        apps: _apps,
        router,
        mut handler,
    } = project;

    let handler = |axum_request: axum::extract::Request| async move {
        let request =
            request_axum_to_flareon(axum_request, Arc::clone(&config), Arc::clone(&router));
        let (request_parts, request) = request_parts_for_diagnostics(request);

        let catch_unwind_response = AssertUnwindSafe(pass_to_axum(request, &mut handler))
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
            let diagnostics = FlareonDiagnostics::new(Arc::clone(&router), request_parts);

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
        std::panic::set_hook(Box::new(error_page::error_page_panic_hook));
    }
    axum::serve(listener, handler.into_make_service())
        .await
        .map_err(|e| ErrorRepr::StartServer { source: e })?;
    if config::REGISTER_PANIC_HOOK {
        let _ = std::panic::take_hook();
    }

    Ok(())
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

fn request_axum_to_flareon(
    axum_request: axum::extract::Request,
    config: Arc<ProjectConfig>,
    router: Arc<Router>,
) -> Request {
    let mut request = axum_request.map(Body::axum);
    prepare_request(&mut request, config, router);
    request
}

pub(crate) fn prepare_request(
    request: &mut Request,
    config: Arc<ProjectConfig>,
    router: Arc<Router>,
) {
    request.extensions_mut().insert(config);
    request.extensions_mut().insert(router);
}

async fn pass_to_axum<S>(request: Request, handler: &mut S) -> Result<axum::response::Response>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
    S::Future: Send,
{
    poll_fn(|cx| handler.poll_ready(cx)).await?;
    let response = handler.call(request).await?;

    Ok(response.map(axum::body::Body::new))
}

/// A trait for types that can be used to render them as HTML.
pub trait Render {
    /// Renders the object as an HTML string.
    fn render(&self) -> Html;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Deref, From)]
pub struct Html(String);

impl Html {
    #[must_use]
    pub fn new<T: Into<String>>(html: T) -> Self {
        Self(html.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures::stream;
    use http_body::Body as HttpBody;

    use super::*;

    #[test]
    fn test_flareon_app_builder() {
        let app = FlareonApp::builder().urls([]).build().unwrap();
        assert!(app.router.is_empty());
    }

    #[test]
    fn test_flareon_project_builder() {
        let app = FlareonApp::builder().urls([]).build().unwrap();
        let mut builder = FlareonProject::builder();
        builder.register_app_with_views(app, "/app");
        let project = builder.build();
        assert_eq!(project.apps.len(), 1);
        assert!(!project.router.is_empty());
    }

    #[test]
    fn test_flareon_project_router() {
        let app = FlareonApp::builder().urls([]).build().unwrap();
        let mut builder = FlareonProject::builder();
        builder.register_app_with_views(app, "/app");
        let project = builder.build();
        assert_eq!(project.router().routes().len(), 1);
    }

    #[test]
    fn test_body_empty() {
        let body = Body::empty();
        if let BodyInner::Fixed(data) = body.inner {
            assert!(data.is_empty());
        } else {
            panic!("Body::empty should create a fixed empty body");
        }
    }

    #[test]
    fn test_body_fixed() {
        let content = "Hello, world!";
        let body = Body::fixed(content);
        if let BodyInner::Fixed(data) = body.inner {
            assert_eq!(data, Bytes::from(content));
        } else {
            panic!("Body::fixed should create a fixed body with the given content");
        }
    }

    #[tokio::test]
    async fn test_body_streaming() {
        let stream = stream::once(async { Ok(Bytes::from("Hello, world!")) });
        let body = Body::streaming(stream);
        if let BodyInner::Streaming(_) = body.inner {
            // Streaming body created successfully
        } else {
            panic!("Body::streaming should create a streaming body");
        }
    }

    #[tokio::test]
    async fn test_http_body_poll_frame_fixed() {
        let content = "Hello, world!";
        let mut body = Body::fixed(content);
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from(content));
            }
            _ => panic!("Body::fixed should return the content in poll_frame"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {} // End of stream
            _ => panic!("Body::fixed should return None after the content is consumed"),
        }
    }

    #[tokio::test]
    async fn test_http_body_poll_frame_streaming() {
        let content = "Hello, world!";
        let mut body = Body::streaming(stream::once(async move { Ok(Bytes::from(content)) }));
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from(content));
            }
            _ => panic!("Body::fixed should return the content in poll_frame"),
        }

        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(None) => {} // End of stream
            _ => panic!("Body::fixed should return None after the content is consumed"),
        }
    }

    #[test]
    fn test_http_body_is_end_stream() {
        let body = Body::empty();
        assert!(body.is_end_stream());

        let body = Body::fixed("Hello, world!");
        assert!(!body.is_end_stream());
    }

    #[test]
    fn test_http_body_size_hint() {
        let body = Body::empty();
        assert_eq!(body.size_hint().exact(), Some(0));

        let content = "Hello, world!";
        let body = Body::fixed(content);
        assert_eq!(body.size_hint().exact(), Some(content.len() as u64));
    }
}
