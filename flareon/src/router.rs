use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use axum::http::StatusCode;
use bytes::Bytes;
use log::debug;

use crate::request::Request;
use crate::router::path::{PathMatcher, ReverseParamMap};
use crate::{Body, Error, RequestHandler, Response, Result};

pub mod path;

#[derive(Clone, Debug)]
pub struct Router {
    urls: Vec<Route>,
    names: HashMap<String, Arc<PathMatcher>>,
}

impl Router {
    #[must_use]
    pub fn with_urls<T: Into<Vec<Route>>>(urls: T) -> Self {
        let urls = urls.into();
        let mut names = HashMap::new();

        for url in &urls {
            if let Some(name) = &url.name {
                names.insert(name.clone(), url.url.clone());
            }
        }

        Self { urls, names }
    }

    async fn route(&self, mut request: Request, request_path: &str) -> Result<Response> {
        debug!("Routing request to {}", request_path);

        for route in &self.urls {
            if let Some(matches) = route.url.capture(request_path) {
                let matches_fully = matches.matches_fully();
                for param in matches.params {
                    request
                        .path_params
                        .insert(param.name.to_owned(), param.value);
                }

                match &route.view {
                    RouteInner::Handler(handler) => {
                        if matches_fully {
                            return handler.handle(request).await;
                        }
                    }
                    RouteInner::Router(router) => {
                        return Box::pin(router.route(request, matches.remaining_path)).await
                    }
                }
            }
        }

        debug!("Not found: {}", request_path);
        Ok(handle_not_found())
    }

    pub async fn handle(&self, request: Request) -> Result<Response> {
        let path = request.uri().path().to_owned();
        self.route(request, &path).await
    }

    /// Get a URL for a view by name.
    ///
    /// Instead of using this method directly, consider using the
    /// [`reverse!`](crate::reverse) macro which provides much more
    /// ergonomic way to call this.
    pub fn reverse(&self, name: &str, params: &ReverseParamMap) -> Result<String> {
        self.reverse_option(name, params)?
            .ok_or_else(|| Error::NoViewToReverse {
                view_name: name.to_owned(),
            })
    }

    pub fn reverse_option(&self, name: &str, params: &ReverseParamMap) -> Result<Option<String>> {
        let url = self.names.get(name).map(|matcher| matcher.reverse(params));
        if let Some(url) = url {
            return Ok(Some(url?));
        }

        for route in &self.urls {
            if let RouteInner::Router(router) = &route.view {
                if let Some(url) = router.reverse_option(name, params)? {
                    return Ok(Some(route.url.reverse(params)? + &url));
                }
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct Route {
    url: Arc<PathMatcher>,
    view: RouteInner,
    name: Option<String>,
}

impl Route {
    #[must_use]
    pub fn with_handler(url: &str, view: Arc<Box<dyn RequestHandler + Send + Sync>>) -> Self {
        Self {
            url: Arc::new(PathMatcher::new(url)),
            view: RouteInner::Handler(view),
            name: None,
        }
    }

    #[must_use]
    pub fn with_handler_and_name<T: Into<String>>(
        url: &str,
        view: Arc<Box<dyn RequestHandler + Send + Sync>>,
        name: T,
    ) -> Self {
        Self {
            url: Arc::new(PathMatcher::new(url)),
            view: RouteInner::Handler(view),
            name: Some(name.into()),
        }
    }

    #[must_use]
    pub fn with_router(url: &str, router: Router) -> Self {
        Self {
            url: Arc::new(PathMatcher::new(url)),
            view: RouteInner::Router(router),
            name: None,
        }
    }
}

fn handle_not_found() -> Response {
    Response::new_html(
        StatusCode::NOT_FOUND,
        Body::Fixed(Bytes::from("404 Not Found")),
    )
}

#[derive(Clone)]
enum RouteInner {
    Handler(Arc<Box<dyn RequestHandler + Send + Sync>>),
    Router(Router),
}

impl Debug for RouteInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self {
            RouteInner::Handler(_) => f.debug_tuple("Handler").field(&"handler(...)").finish(),
            RouteInner::Router(router) => f.debug_tuple("Router").field(router).finish(),
        }
    }
}

#[macro_export]
macro_rules! reverse {
    ($request:expr, $view_name:literal $(, $($key:expr => $value:expr),* )?) => {
        ::flareon::Response::new_redirect($crate::reverse_str!(
            $request,
            $view_name,
            $( $($key => $value),* )?
        ))
    };
}

#[macro_export]
macro_rules! reverse_str {
    ( $request:expr, $view_name:literal $(, $($key:expr => $value:expr),* )? ) => {
        $request
            .project()
            .router()
            .reverse($view_name, &$crate::reverse_param_map!($( $($key => $value),* )?))?
    };
}
