extern crate self as flareon;

mod error;
pub mod forms;
pub mod prelude;
#[doc(hidden)]
pub mod private;
pub mod request;
pub mod router;

use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::io::Read;
use std::sync::Arc;

use async_trait::async_trait;
use axum::handler::HandlerWithoutStateExt;
use bytes::Bytes;
use derive_builder::Builder;
pub use error::Error;
use indexmap::IndexMap;
use log::info;
use request::Request;
use router::{Route, Router};

pub type Result<T> = std::result::Result<T, crate::Error>;

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

const CONTENT_TYPE_HEADER: &str = "Content-Type";
const HTML_CONTENT_TYPE: &str = "text/html";
const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";
const LOCATION_HEADER: &str = "Location";

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
    Streaming(Box<dyn Read>),
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

    #[must_use]
    pub fn register_app_with_views(&mut self, app: FlareonApp, url_prefix: &str) -> &mut Self {
        let new = self;
        new.urls
            .push(Route::with_router(url_prefix, app.router.clone()));
        new.apps.push(app);
        new
    }

    pub fn build(&self) -> Result<FlareonProject> {
        Ok(FlareonProject {
            apps: self.apps.clone(),
            router: Router::with_urls(self.urls.clone()),
        })
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

pub async fn run(mut project: FlareonProject, address_str: &str) -> Result<()> {
    for app in &mut project.apps {
        info!("Initializing app: {:?}", app);
    }

    let project = Arc::new(project);
    let listener = tokio::net::TcpListener::bind(address_str).await.unwrap();

    let handler = |request: axum::extract::Request| async move {
        pass_to_axum(&project, Request::new(request, project.clone()))
            .await
            .unwrap_or_else(handle_response_error)
    };
    axum::serve(listener, handler.into_make_service())
        .await
        .unwrap();

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
    let axum_response = builder.body(match response.body {
        Body::Fixed(data) => axum::body::Body::from(data),
        Body::Streaming(_) => unimplemented!(),
    });

    match axum_response {
        Ok(response) => Ok(response),
        Err(error) => Err(Error::ResponseBuilder(error)),
    }
}

fn handle_response_error(_error: Error) -> axum::response::Response {
    unimplemented!("500 error handler is not implemented yet")
}
