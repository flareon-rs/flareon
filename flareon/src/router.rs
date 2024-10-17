//! Router for passing requests to their respective views.

use std::collections::HashMap;
use std::fmt::Formatter;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::http::StatusCode;
use bytes::Bytes;
use derive_more::Debug;
use flareon::request::PathParams;
use log::debug;

use crate::error::ErrorRepr;
use crate::error_page::ErrorPageTrigger;
use crate::request::Request;
use crate::response::{Response, ResponseExt};
use crate::router::path::{CaptureResult, PathMatcher, ReverseParamMap};
use crate::{Body, Error, RequestHandler, Result};

pub mod path;

#[derive(Clone, Debug)]
pub struct Router {
    urls: Vec<Route>,
    names: HashMap<String, Arc<PathMatcher>>,
}

impl Router {
    #[must_use]
    pub fn empty() -> Self {
        Self::with_urls(&[])
    }

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

        if let Some(result) = self.get_handler(request_path) {
            let mut path_params = PathParams::new();
            for (key, value) in result.params.iter().rev() {
                path_params.insert(key.clone(), value.clone());
            }
            request.extensions_mut().insert(path_params);
            result.handler.handle(request).await
        } else {
            debug!("Not found: {}", request_path);
            Ok(handle_not_found())
        }
    }

    fn get_handler(&self, request_path: &str) -> Option<HandlerFound> {
        for route in &self.urls {
            if let Some(matches) = route.url.capture(request_path) {
                let matches_fully = matches.matches_fully();

                match &route.view {
                    RouteInner::Handler(handler) => {
                        if matches_fully {
                            return Some(HandlerFound {
                                handler: &**handler,
                                params: Self::matches_to_path_params(&matches, Vec::new()),
                            });
                        }
                    }
                    RouteInner::Router(router) => {
                        if let Some(result) = router.get_handler(matches.remaining_path) {
                            return Some(HandlerFound {
                                handler: result.handler,
                                params: Self::matches_to_path_params(&matches, result.params),
                            });
                        }
                    }
                }
            }
        }

        None
    }

    fn matches_to_path_params(
        matches: &CaptureResult,
        mut path_params: Vec<(String, String)>,
    ) -> Vec<(String, String)> {
        // Adding in reverse order, since we're doing this from the bottom up (we're
        // going to reverse the order before running the handler)
        for param in matches.params.iter().rev() {
            path_params.push((param.name.to_owned(), param.value.clone()));
        }
        path_params
    }

    /// Handle a request.
    ///
    /// This method is called by the [`FlareonApp`](crate::FlareonApp) to handle
    /// a request.
    ///
    /// # Errors
    ///
    /// This method re-throws any errors that occur in the request handler.
    pub async fn handle(&self, request: Request) -> Result<Response> {
        let path = request.uri().path().to_owned();
        self.route(request, &path).await
    }

    /// Get a URL for a view by name.
    ///
    /// Instead of using this method directly, consider using the
    /// [`reverse!`](crate::reverse) macro which provides much more
    /// ergonomic way to call this.
    ///
    /// # Errors
    ///
    /// This method returns an error if the view name is not found.
    ///
    /// This method returns an error if the URL cannot be generated because of
    /// missing parameters.
    pub fn reverse(&self, name: &str, params: &ReverseParamMap) -> Result<String> {
        Ok(self
            .reverse_option(name, params)?
            .ok_or_else(|| ErrorRepr::NoViewToReverse {
                view_name: name.to_owned(),
            })?)
    }

    /// Get a URL for a view by name.
    ///
    /// Returns `None` if the view name is not found.
    ///
    /// # Errors
    ///
    /// This method returns an error if the URL cannot be generated because of
    /// missing parameters.
    pub fn reverse_option(&self, name: &str, params: &ReverseParamMap) -> Result<Option<String>> {
        let url = self.names.get(name).map(|matcher| matcher.reverse(params));
        if let Some(url) = url {
            return Ok(Some(url.map_err(ErrorRepr::from)?));
        }

        for route in &self.urls {
            if let RouteInner::Router(router) = &route.view {
                if let Some(url) = router.reverse_option(name, params)? {
                    return Ok(Some(
                        route.url.reverse(params).map_err(ErrorRepr::from)? + &url,
                    ));
                }
            }
        }
        Ok(None)
    }

    #[must_use]
    pub fn routes(&self) -> &[Route] {
        &self.urls
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug)]
struct HandlerFound<'a> {
    #[debug("handler(...)")]
    handler: &'a (dyn RequestHandler + Send + Sync),
    params: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct RouterService {
    router: Arc<Router>,
}

impl RouterService {
    #[must_use]
    pub fn new(router: Arc<Router>) -> Self {
        Self { router }
    }
}

impl tower::Service<Request> for RouterService {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;
    type Response = Response;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let router = self.router.clone();
        Box::pin(async move { router.handle(req).await })
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
    pub fn with_handler<V: RequestHandler + Send + Sync + 'static>(url: &str, view: V) -> Self {
        Self {
            url: Arc::new(PathMatcher::new(url)),
            view: RouteInner::Handler(Arc::new(view)),
            name: None,
        }
    }

    #[must_use]
    pub fn with_handler_and_name<T: Into<String>, V: RequestHandler + Send + Sync + 'static>(
        url: &str,
        view: V,
        name: T,
    ) -> Self {
        Self {
            url: Arc::new(PathMatcher::new(url)),
            view: RouteInner::Handler(Arc::new(view)),
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

    #[must_use]
    pub fn url(&self) -> String {
        self.url.to_string()
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub(crate) fn kind(&self) -> RouteKind {
        match &self.view {
            RouteInner::Handler(_) => RouteKind::Handler,
            RouteInner::Router(_) => RouteKind::Router,
        }
    }

    #[must_use]
    pub(crate) fn router(&self) -> Option<&Router> {
        match &self.view {
            RouteInner::Router(router) => Some(router),
            _ => None,
        }
    }
}

fn handle_not_found() -> Response {
    let mut response = Response::new_html(
        StatusCode::NOT_FOUND,
        Body::fixed(Bytes::from("404 Not Found")),
    );
    response.extensions_mut().insert(ErrorPageTrigger::NotFound);
    response
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum RouteKind {
    Handler,
    Router,
}

#[derive(Clone)]
enum RouteInner {
    Handler(Arc<dyn RequestHandler + Send + Sync>),
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

/// Reverse a URL for a view by name and given params.
///
/// # Examples
///
/// ```
/// use flareon::request::Request;
/// use flareon::response::{Response, ResponseExt};
/// use flareon::router::{Route, Router};
/// use flareon::{reverse_str, Body, StatusCode};
///
/// async fn home(request: Request) -> flareon::Result<Response> {
///     Ok(Response::new_html(
///         StatusCode::OK,
///         Body::fixed(format!(
///             "Hello! The URL for this view is: {}",
///             reverse_str!(request, "home")
///         )),
///     ))
/// }
///
/// let router = Router::with_urls([Route::with_handler_and_name("/", home, "home")]);
/// ```
#[macro_export]
macro_rules! reverse_str {
    ($request:expr, $view_name:literal $(, $($key:expr => $value:expr),*)?) => {{
        use $crate::request::RequestExt;
        $request
            .router()
            .reverse($view_name, &$crate::reverse_param_map!($( $($key => $value),* )?))?
    }};
}

/// Reverse a URL for a view by name and given params and return a response with
/// a redirect.
///
/// This macro is a shorthand for creating a response with a redirect to a URL
/// generated by the [`reverse_str!`] macro.
///
/// # Examples
///
/// ```
/// use flareon::request::Request;
/// use flareon::response::Response;
/// use flareon::reverse;
/// use flareon::router::{Route, Router};
///
/// async fn infinite_loop(request: Request) -> flareon::Result<Response> {
///     Ok(reverse!(request, "home"))
/// }
///
/// let router = Router::with_urls([Route::with_handler_and_name("/", infinite_loop, "home")]);
/// ```
#[macro_export]
macro_rules! reverse {
    ($request:expr, $view_name:literal $(, $($key:expr => $value:expr),*)?) => {
        <$crate::response::Response as $crate::response::ResponseExt>::new_redirect(
            $crate::reverse_str!(
                $request,
                $view_name,
                $( $($key => $value),* )?
            )
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Request;
    use crate::test::TestRequestBuilder;
    use crate::Response;

    struct MockHandler;

    #[async_trait::async_trait]
    impl RequestHandler for MockHandler {
        async fn handle(&self, _request: Request) -> Result<Response> {
            Ok(Response::new_html(
                StatusCode::OK,
                Body::fixed(Bytes::from("OK")),
            ))
        }
    }

    #[test]
    fn router_with_urls() {
        let route = Route::with_handler("/test", MockHandler);
        let router = Router::with_urls(vec![route.clone()]);
        assert_eq!(router.routes().len(), 1);
    }

    #[tokio::test]
    async fn router_route() {
        let route = Route::with_handler("/test", MockHandler);
        let router = Router::with_urls(vec![route.clone()]);
        let response = router.route(test_request(), "/test").await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn router_handle() {
        let route = Route::with_handler("/test", MockHandler);
        let router = Router::with_urls(vec![route.clone()]);
        let response = router.handle(test_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn sub_router_handle() {
        let route_1 = Route::with_handler("/test", MockHandler);
        let sub_router_1 = Router::with_urls(vec![route_1.clone()]);
        let route_2 = Route::with_handler("/test", MockHandler);
        let sub_router_2 = Router::with_urls(vec![route_2.clone()]);

        let router = Router::with_urls(vec![
            Route::with_router("/", sub_router_1),
            Route::with_router("/sub", sub_router_2),
        ]);
        let response = router
            .handle(TestRequestBuilder::get("/sub/test").build())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn router_reverse() {
        let route = Route::with_handler_and_name("/test", MockHandler, "test");
        let router = Router::with_urls(vec![route.clone()]);
        let params = ReverseParamMap::new();
        let url = router.reverse("test", &params).unwrap();
        assert_eq!(url, "/test");
    }

    #[test]
    fn router_reverse_with_param() {
        let route = Route::with_handler_and_name("/test/:id", MockHandler, "test");
        let router = Router::with_urls(vec![route.clone()]);
        let mut params = ReverseParamMap::new();
        params.insert("id", "123");
        let url = router.reverse("test", &params).unwrap();
        assert_eq!(url, "/test/123");
    }

    #[test]
    fn router_reverse_option() {
        let route = Route::with_handler_and_name("/test", MockHandler, "test");
        let router = Router::with_urls(vec![route.clone()]);
        let params = ReverseParamMap::new();
        let url = router.reverse_option("test", &params).unwrap().unwrap();
        assert_eq!(url, "/test");
    }

    #[test]
    fn router_routes() {
        let route = Route::with_handler("/test", MockHandler);
        let router = Router::with_urls(vec![route.clone()]);
        assert_eq!(router.routes().len(), 1);
    }

    #[test]
    fn router_is_empty() {
        let router = Router::with_urls(vec![]);
        assert!(router.is_empty());
    }

    #[test]
    fn route_with_handler() {
        let route = Route::with_handler("/test", MockHandler);
        assert_eq!(route.url.to_string(), "/test");
    }

    #[test]
    fn route_with_handler_and_params() {
        let route = Route::with_handler("/test/:id", MockHandler);
        assert_eq!(route.url.to_string(), "/test/:id");
    }

    #[test]
    fn route_with_handler_and_name() {
        let route = Route::with_handler_and_name("/test", MockHandler, "test");
        assert_eq!(route.url.to_string(), "/test");
        assert_eq!(route.name.as_deref(), Some("test"));
    }

    #[test]
    fn route_with_router() {
        let sub_route = Route::with_handler("/sub", MockHandler);
        let sub_router = Router::with_urls(vec![sub_route]);
        let route = Route::with_router("/test", sub_router);
        assert_eq!(route.url.to_string(), "/test");
    }

    fn test_request() -> Request {
        TestRequestBuilder::get("/test").build()
    }
}
