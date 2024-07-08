pub mod prelude;

use std::fmt::{Debug, Formatter};
use std::io::Read;
use std::sync::Arc;

use async_trait::async_trait;
use axum::handler::HandlerWithoutStateExt;
use bytes::Bytes;
use derive_builder::Builder;
use indexmap::IndexMap;
use log::info;
use thiserror::Error;

pub type StatusCode = axum::http::StatusCode;

#[async_trait]
pub trait RequestHandler {
    async fn handle(&self, request: Request) -> Result<Response, Error>;
}

#[derive(Clone, Debug)]
pub struct Router {
    urls: Vec<Route>,
}

impl Router {
    #[must_use]
    pub fn with_urls<T: Into<Vec<Route>>>(urls: T) -> Self {
        Self { urls: urls.into() }
    }

    async fn route(&self, request: Request, request_path: &str) -> Result<Response, Error> {
        for route in &self.urls {
            if request_path.starts_with(&route.url) {
                let request_path = &request_path[route.url.len()..];
                match &route.view {
                    RouteInner::Handler(handler) => return handler.handle(request).await,
                    RouteInner::Router(router) => {
                        return Box::pin(router.route(request, request_path)).await
                    }
                }
            }
        }

        unimplemented!("404 handler is not implemented yet")
    }
}

#[async_trait]
impl RequestHandler for Router {
    async fn handle(&self, request: Request) -> Result<Response, Error> {
        let path = request.uri().path().to_owned();
        self.route(request, &path).await
    }
}

#[async_trait]
impl<T> RequestHandler for T
where
    T: Fn(Request) -> Result<Response, Error> + Send + Sync,
{
    async fn handle(&self, request: Request) -> Result<Response, Error> {
        self(request)
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

#[derive(Clone)]
pub struct Route {
    url: String,
    view: RouteInner,
}

impl Route {
    #[must_use]
    pub fn with_handler<T: Into<String>>(
        url: T,
        view: Arc<Box<dyn RequestHandler + Send + Sync>>,
    ) -> Self {
        Self {
            url: url.into(),
            view: RouteInner::Handler(view),
        }
    }

    #[must_use]
    pub fn with_router<T: Into<String>>(url: T, router: Router) -> Self {
        Self {
            url: url.into(),
            view: RouteInner::Router(router),
        }
    }
}

#[derive(Clone)]
enum RouteInner {
    Handler(Arc<Box<dyn RequestHandler + Send + Sync>>),
    Router(Router),
}

impl Debug for Route {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.view {
            RouteInner::Handler(_) => f.debug_tuple("Handler").field(&"handler(...)").finish(),
            RouteInner::Router(router) => f.debug_tuple("Router").field(router).finish(),
        }
    }
}

pub type Request = axum::extract::Request;

type HeadersMap = IndexMap<String, String>;

#[derive(Debug)]
pub struct Response {
    status: StatusCode,
    headers: HeadersMap,
    body: Body,
}

const CONTENT_TYPE_HEADER: &str = "Content-Type";
const HTML_CONTENT_TYPE: &str = "text/html";

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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not create a response object: {0}")]
    ResponseBuilder(#[from] axum::http::Error),
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
        new.urls.push(Route::with_handler(
            url_prefix,
            Arc::new(Box::new(app.router.clone())),
        ));
        new.apps.push(app);
        new
    }

    pub fn build(&self) -> Result<FlareonProject, Error> {
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
}

pub async fn run(mut project: FlareonProject, address_str: &str) -> Result<(), Error> {
    for app in &mut project.apps {
        info!("Initializing app: {:?}", app);
    }

    let listener = tokio::net::TcpListener::bind(address_str).await.unwrap();

    let handler = |request: axum::extract::Request| async move {
        pass_to_axum(&project, request)
            .await
            .unwrap_or_else(handle_response_error)
    };
    axum::serve(listener, handler.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn pass_to_axum(
    project: &FlareonProject,
    request: axum::extract::Request,
) -> Result<axum::response::Response, Error> {
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
