use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use bytes::Bytes;
use log::debug;

use crate::request::Request;
use crate::router::path::PathMatcher;
use crate::{Body, Error, RequestHandler, Response, RouteInner};

mod path;

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
        debug!("Routing request to {}", request_path);

        for route in &self.urls {
            if let Some(matches) = route.url.capture(request_path) {
                match &route.view {
                    RouteInner::Handler(handler) => {
                        if matches.matches_fully() {
                            return handler.handle(request).await;
                        }
                    }
                    RouteInner::Router(router) => {
                        return Box::pin(router.route(request, matches.remaining_path())).await
                    }
                }
            }
        }

        debug!("Not found: {}", request_path);
        Ok(handle_not_found())
    }

    pub async fn handle(&self, request: Request) -> Result<Response, Error> {
        let path = request.uri().path().to_owned();
        self.route(request, &path).await
    }
}

#[derive(Clone)]
pub struct Route {
    url: PathMatcher,
    view: RouteInner,
}

impl Route {
    #[must_use]
    pub fn with_handler(url: &str, view: Arc<Box<dyn RequestHandler + Send + Sync>>) -> Self {
        Self {
            url: PathMatcher::new(url),
            view: RouteInner::Handler(view),
        }
    }

    #[must_use]
    pub fn with_router(url: &str, router: Router) -> Self {
        Self {
            url: PathMatcher::new(url),
            view: RouteInner::Router(router),
        }
    }
}

impl Debug for Route {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.view {
            RouteInner::Handler(_) => f.debug_tuple("Handler").field(&"handler(...)").finish(),
            RouteInner::Router(router) => f.debug_tuple("Router").field(router).finish(),
        }
    }
}

fn handle_not_found() -> Response {
    Response::new_html(
        StatusCode::NOT_FOUND,
        Body::Fixed(Bytes::from("404 Not Found")),
    )
}
