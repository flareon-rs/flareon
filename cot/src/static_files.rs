//! Static files middleware.
//!
//! This middleware serves static files from the `static` directory of the
//! project.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::ready;
use http::{header, Request};
use pin_project_lite::pin_project;
use tower::Service;

use crate::project::WithApps;
use crate::response::{Response, ResponseExt};
use crate::{Body, ProjectContext};

/// Macro to define static files by specifying their paths.
///
/// The files are included at compile time using the `include_bytes!` macro.
/// The paths are relative to the `static` directory of the project (under the
/// project root, where the `Cargo.toml` file is).
///
/// This is mainly useful with the
/// [`CotApp::static_files`](crate::App::static_files) trait method.
///
/// # Example
///
/// ```
/// use bytes::Bytes;
/// use cot::{static_files, App};
///
/// pub struct ExampleApp;
///
/// // Project structure:
/// // .
/// // ├── Cargo.toml
/// // └── static
/// //     └── admin
/// //         └── admin.css
///
/// impl App for ExampleApp {
///     fn name(&self) -> &str {
///         "test_app"
///     }
///
///     fn static_files(&self) -> Vec<(String, Bytes)> {
///         static_files!("admin/admin.css")
///     }
/// }
/// ```
#[macro_export]
macro_rules! static_files {
    ($($path:literal),*) => {
        vec![$(
            (
                $path.to_string(),
                $crate::__private::Bytes::from_static(
                    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/", $path))
                ),
            )
        ),*]
    };
}

/// Struct representing a collection of static files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaticFiles {
    files: HashMap<String, File>,
}

impl StaticFiles {
    /// Creates a new `StaticFiles` instance.
    #[must_use]
    fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    fn add_file(&mut self, path: &str, content: impl Into<Bytes>) {
        let mime_type = mime_guess::from_path(path).first_or_octet_stream();
        let file = File::new(content, mime_type);
        self.files.insert(path.to_string(), file);
    }

    #[must_use]
    fn get_file(&self, path: &str) -> Option<&File> {
        self.files.get(path)
    }

    pub(crate) fn collect_into(&self, path: &Path) -> Result<(), std::io::Error> {
        for (file_path, file) in &self.files {
            let file_path = path.join(file_path);
            std::fs::create_dir_all(
                file_path
                    .parent()
                    .expect("joined file path should always have a parent"),
            )?;
            std::fs::write(file_path, &file.content)?;
        }
        Ok(())
    }
}

impl Default for StaticFiles {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&ProjectContext<WithApps>> for StaticFiles {
    fn from(app_context: &ProjectContext<WithApps>) -> Self {
        let mut static_files = StaticFiles::new();

        for module in app_context.apps() {
            for (path, content) in module.static_files() {
                static_files.add_file(&path, content);
            }
        }

        static_files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct File {
    content: Bytes,
    mime_type: mime_guess::Mime,
}

impl File {
    #[must_use]
    fn new(content: impl Into<Bytes>, mime_type: mime_guess::Mime) -> Self {
        Self {
            content: content.into(),
            mime_type,
        }
    }

    #[must_use]
    fn as_response(&self) -> Response {
        Response::builder()
            .header(header::CONTENT_TYPE, self.mime_type.to_string())
            .body(Body::fixed(self.content.clone()))
            .expect("failed to build static file response")
    }
}

/// Middleware for serving static files.
///
/// This middleware serves static files defined by the applications by using
/// the [`CotApp::static_files`](crate::App::static_files) trait
/// method. The middleware serves files from the `/static/` path.
///
/// If a request is made to a path starting with `/static/`, the middleware
/// checks if the file exists in the static files collection. If it does, the
/// file is served. Otherwise, the request is passed to the inner service.
#[derive(Debug, Clone)]
pub struct StaticFilesMiddleware {
    static_files: Arc<StaticFiles>,
}

impl StaticFilesMiddleware {
    /// Creates a new `StaticFilesMiddleware` instance from the application
    /// context.
    #[must_use]
    pub fn from_app_context(app_context: &ProjectContext<WithApps>) -> Self {
        Self {
            static_files: Arc::new(StaticFiles::from(app_context)),
        }
    }
}

impl<S> tower::Layer<S> for StaticFilesMiddleware {
    type Service = StaticFilesService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        StaticFilesService::new(Arc::clone(&self.static_files), inner)
    }
}

/// Service for handling requests and serving static files.
#[derive(Clone, Debug)]
pub struct StaticFilesService<S> {
    static_files: Arc<StaticFiles>,
    inner: S,
}

impl<S> StaticFilesService<S> {
    /// Create a new static files service.
    #[must_use]
    fn new(static_files: Arc<StaticFiles>, inner: S) -> Self {
        Self {
            static_files,
            inner,
        }
    }
}

impl<ReqBody, S> Service<Request<ReqBody>> for StaticFilesService<S>
where
    S: Service<Request<ReqBody>, Response = Response>,
{
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;
    type Response = S::Response;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        const STATIC_PATH: &str = "/static/";

        let path = req.uri().path();
        let file_contents = if let Some(stripped_path) = path.strip_prefix(STATIC_PATH) {
            self.static_files
                .get_file(stripped_path)
                .map(File::as_response)
        } else {
            None
        };

        match file_contents {
            Some(response) => ResponseFuture::StaticFileResponse { response },
            None => ResponseFuture::Inner {
                future: self.inner.call(req),
            },
        }
    }
}

pin_project! {
    /// Future representing the response for a static file request.
    #[project = ResponseFutureProj]
    #[allow(missing_docs)]  // because of: https://github.com/taiki-e/pin-project-lite/issues/3
    pub enum ResponseFuture<F> {
        /// Response for a static file.
        StaticFileResponse {
            // A [`Response`] object for a static file.
            response: Response,
        },
        /// Response from the inner service.
        Inner {
            // The inner service's future.
            #[pin]
            future: F,
        },
    }
}

impl<F, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response, E>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this {
            ResponseFutureProj::StaticFileResponse { response } => {
                Poll::Ready(Ok(std::mem::take(response)))
            }
            ResponseFutureProj::Inner { future } => {
                let res = ready!(future.poll(cx)?);
                Poll::Ready(Ok(res))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use http::{Request, StatusCode};
    use tower::{Layer, ServiceExt};

    use super::*;
    use crate::config::ProjectConfig;
    use crate::project::WithConfig;
    use crate::{App, AppBuilder, Bootstrapper, Project};

    #[test]
    #[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
    fn static_files_add_and_get_file() {
        let mut static_files = StaticFiles::new();
        static_files.add_file("test.txt", "This is a test file");

        let file = static_files.get_file("test.txt");

        assert!(file.is_some());
        assert_eq!(file.unwrap().content, Bytes::from("This is a test file"));
    }

    #[tokio::test]
    async fn file_as_response() {
        let file = File::new("This is a test file", mime_guess::mime::TEXT_PLAIN);

        let response = file.as_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/plain");
        assert_eq!(
            response.into_body().into_bytes().await.unwrap(),
            Bytes::from("This is a test file")
        );
    }

    fn create_static_files() -> StaticFiles {
        let mut static_files = StaticFiles::new();
        static_files.add_file("test.txt", "This is a test file");
        static_files
    }

    #[tokio::test]
    async fn static_files_middleware() {
        let static_files = Arc::new(create_static_files());
        let middleware = StaticFilesMiddleware {
            static_files: Arc::clone(&static_files),
        };

        let service = middleware.layer(tower::service_fn(|_req| async {
            Ok::<_, std::convert::Infallible>(Response::new(Body::empty()))
        }));

        let request = Request::builder()
            .uri("/static/test.txt")
            .body(Body::empty())
            .unwrap();

        let response = service.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/plain");
        assert_eq!(
            response.into_body().into_bytes().await.unwrap(),
            Bytes::from("This is a test file")
        );
    }

    #[tokio::test]
    async fn static_files_middleware_not_found() {
        let static_files = Arc::new(create_static_files());
        let middleware = StaticFilesMiddleware {
            static_files: Arc::clone(&static_files),
        };
        let service = middleware.layer(tower::service_fn(|_req| async {
            Ok::<_, std::convert::Infallible>(Response::new(Body::fixed("test")))
        }));

        let request = Request::builder()
            .uri("/static/nonexistent.txt")
            .body(Body::empty())
            .unwrap();

        let response = service.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().into_bytes().await.unwrap(),
            Bytes::from("test") // Inner service response
        );
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
    async fn static_files_middleware_from_app_context() {
        struct App1;
        impl App for App1 {
            fn name(&self) -> &'static str {
                "app1"
            }

            fn static_files(&self) -> Vec<(String, Bytes)> {
                static_files!("admin/admin.css")
            }
        }

        struct App2;
        impl App for App2 {
            fn name(&self) -> &'static str {
                "app2"
            }

            fn static_files(&self) -> Vec<(String, Bytes)> {
                vec![("app2/test.js".to_string(), Bytes::from("test"))]
            }
        }

        struct TestProject;
        impl Project for TestProject {
            fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {
                apps.register(App1);
                apps.register(App2);
            }
        }

        let bootstrapper = Bootstrapper::new(TestProject)
            .with_config(ProjectConfig::default())
            .with_apps();
        let middleware = StaticFilesMiddleware::from_app_context(bootstrapper.context());
        let static_files = middleware.static_files;

        let file = static_files.get_file("admin/admin.css").unwrap();
        assert_eq!(file.mime_type, mime_guess::mime::TEXT_CSS);
        assert_eq!(
            file.content,
            Bytes::from_static(include_bytes!("../static/admin/admin.css"))
        );

        let file = static_files.get_file("app2/test.js").unwrap();
        assert_eq!(file.content, Bytes::from("test"));
    }

    #[test]
    fn collect_into() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let mut static_files = StaticFiles::new();
        static_files.add_file("test.txt", "This is a test file");
        static_files.add_file("nested/test2.txt", "This is another test file");

        static_files.collect_into(&temp_path).unwrap();

        let file_path = temp_path.join("test.txt");
        let nested_file_path = temp_path.join("nested/test2.txt");

        assert!(file_path.exists());
        assert_eq!(
            fs::read_to_string(file_path).unwrap(),
            "This is a test file"
        );

        assert!(nested_file_path.exists());
        assert_eq!(
            fs::read_to_string(nested_file_path).unwrap(),
            "This is another test file"
        );
    }

    #[test]
    fn collect_into_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let static_files = StaticFiles::new();
        static_files.collect_into(&temp_path).unwrap();

        assert!(fs::read_dir(&temp_path).unwrap().next().is_none());
    }
}
