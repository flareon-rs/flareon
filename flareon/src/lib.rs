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
#[doc(hidden)]
pub mod private;
pub mod request;
pub mod router;

use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use axum::handler::HandlerWithoutStateExt;
use bytes::Bytes;
use derive_builder::Builder;
use derive_more::{Deref, From};
pub use error::Error;
use futures_core::Stream;
use headers::{CONTENT_TYPE_HEADER, HTML_CONTENT_TYPE, LOCATION_HEADER};
use http_body::{Frame, SizeHint};
use indexmap::IndexMap;
use log::info;
use request::Request;
use router::{Route, Router};

/// A type alias for a result that can return a `flareon::Error`.
pub type Result<T> = std::result::Result<T, Error>;

/// A type alias for an HTTP status code.
pub type StatusCode = axum::http::StatusCode;

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
        self.router = Some(Router::with_urls(urls.into()));
        self
    }
}

type HeadersMap = IndexMap<String, String>;

#[derive(Debug)]
pub struct Response {
    status: StatusCode,
    headers: HeadersMap,
    body: Body,
}

impl Response {
    #[must_use]
    pub fn new_html(status: StatusCode, body: Body) -> Self {
        Self {
            status,
            headers: Self::html_headers(),
            body,
        }
    }

    #[must_use]
    pub fn new_redirect<T: Into<String>>(location: T) -> Self {
        let mut headers = HeadersMap::new();
        headers.insert(LOCATION_HEADER.to_owned(), location.into());
        Self {
            status: StatusCode::SEE_OTHER,
            headers,
            body: Body::empty(),
        }
    }

    #[must_use]
    fn html_headers() -> HeadersMap {
        let mut headers = HeadersMap::new();
        headers.insert(CONTENT_TYPE_HEADER.to_owned(), HTML_CONTENT_TYPE.to_owned());
        headers
    }
}

pub enum Body {
    Fixed(Bytes),
    Streaming(Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>),
}

impl Debug for Body {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Body::Fixed(data) => f.debug_tuple("Fixed").field(data).finish(),
            Body::Streaming(_) => f.debug_tuple("Streaming").field(&"...").finish(),
        }
    }
}

impl Body {
    #[must_use]
    pub fn empty() -> Self {
        Self::Fixed(Bytes::new())
    }

    #[must_use]
    pub fn fixed<T: Into<Bytes>>(data: T) -> Self {
        Self::Fixed(data.into())
    }

    #[must_use]
    pub fn streaming<T: Stream<Item = Result<Bytes>> + Send + 'static>(stream: T) -> Self {
        Self::Streaming(Box::pin(stream))
    }
}

impl http_body::Body for Body {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut() {
            Body::Fixed(ref mut data) => {
                if data.is_empty() {
                    Poll::Ready(None)
                } else {
                    let data = std::mem::take(data);
                    Poll::Ready(Some(Ok(Frame::data(data))))
                }
            }
            Body::Streaming(ref mut stream) => {
                let stream = Pin::as_mut(stream);
                match stream.poll_next(cx) {
                    Poll::Ready(Some(result)) => Poll::Ready(Some(result.map(Frame::data))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Body::Fixed(data) => data.is_empty(),
            Body::Streaming(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Body::Fixed(data) => SizeHint::with_exact(data.len() as u64),
            Body::Streaming(_) => SizeHint::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FlareonProject {
    apps: Vec<FlareonApp>,
    router: Router,
}

#[derive(Debug)]
pub struct FlareonProjectBuilder {
    apps: Vec<FlareonApp>,
    urls: Vec<Route>,
}

impl FlareonProjectBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            apps: Vec::new(),
            urls: Vec::new(),
        }
    }

    pub fn register_app_with_views(&mut self, app: FlareonApp, url_prefix: &str) -> &mut Self {
        let new = self;
        new.urls
            .push(Route::with_router(url_prefix, app.router.clone()));
        new.apps.push(app);
        new
    }

    /// Builds the Flareon project instance.
    #[must_use]
    pub fn build(&self) -> FlareonProject {
        FlareonProject {
            apps: self.apps.clone(),
            router: Router::with_urls(self.urls.clone()),
        }
    }
}

impl Default for FlareonProjectBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FlareonProject {
    #[must_use]
    pub fn builder() -> FlareonProjectBuilder {
        FlareonProjectBuilder::default()
    }

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
pub async fn run(project: FlareonProject, address_str: &str) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(address_str)
        .await
        .map_err(|e| Error::StartServer { source: e })?;

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
pub async fn run_at(mut project: FlareonProject, listener: tokio::net::TcpListener) -> Result<()> {
    for app in &mut project.apps {
        info!("Initializing app: {:?}", app);
    }

    let project = Arc::new(project);

    let handler = |request: axum::extract::Request| async move {
        pass_to_axum(&project, Request::new(request, project.clone()))
            .await
            .unwrap_or_else(handle_response_error)
    };

    eprintln!(
        "Starting the server at http://{}",
        listener
            .local_addr()
            .map_err(|e| Error::StartServer { source: e })?
    );
    axum::serve(listener, handler.into_make_service())
        .await
        .map_err(|e| Error::StartServer { source: e })?;

    Ok(())
}

async fn pass_to_axum(
    project: &Arc<FlareonProject>,
    request: Request,
) -> Result<axum::response::Response> {
    let response = project.router.handle(request).await?;

    let mut builder = axum::http::Response::builder().status(response.status);
    for (key, value) in response.headers {
        builder = builder.header(key, value);
    }
    let axum_response = builder.body(axum::body::Body::new(response.body));

    match axum_response {
        Ok(response) => Ok(response),
        Err(error) => Err(Error::ResponseBuilder(error)),
    }
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

#[allow(clippy::needless_pass_by_value)]
fn handle_response_error(_error: Error) -> axum::response::Response {
    unimplemented!("500 error handler is not implemented yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flareon_app_builder() {
        let app = FlareonApp::builder().urls([]).build().unwrap();
        assert!(app.router.is_empty());
    }

    #[test]
    fn test_response_new_html() {
        let body = Body::fixed("<html></html>");
        let response = Response::new_html(StatusCode::OK, body);
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            response.headers.get(CONTENT_TYPE_HEADER).unwrap(),
            HTML_CONTENT_TYPE
        );
    }

    #[test]
    fn test_response_new_redirect() {
        let location = "http://example.com";
        let response = Response::new_redirect(location);
        assert_eq!(response.status, StatusCode::SEE_OTHER);
        assert_eq!(response.headers.get(LOCATION_HEADER).unwrap(), location);
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
}
