use cot::cli::CliMetadata;
use cot::config::ProjectConfig;
use cot::project::{ErrorPageHandler, WithConfig};
use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::{App, AppBuilder, Body, Project, ProjectContext, StatusCode};

async fn return_hello(_request: Request) -> cot::Result<Response> {
    panic!()
}

struct HelloApp;

impl App for HelloApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", return_hello)])
    }
}

struct HelloProject;

impl Project for HelloProject {
    fn cli_metadata(&self) -> CliMetadata {
        cot::cli::metadata!()
    }

    fn config(&self, _config_name: &str) -> cot::Result<ProjectConfig> {
        let mut config = ProjectConfig::dev_default();
        config.debug = false; // make sure we can see our custom error pages
        config.register_panic_hook = true;
        Ok(config)
    }

    fn register_apps(&self, modules: &mut AppBuilder, _app_context: &ProjectContext<WithConfig>) {
        modules.register_with_views(HelloApp, "");
    }

    fn server_error_handler(&self) -> Box<dyn ErrorPageHandler> {
        Box::new(CustomServerError)
    }

    fn not_found_handler(&self) -> Box<dyn ErrorPageHandler> {
        Box::new(CustomNotFound)
    }
}

struct CustomServerError;
impl ErrorPageHandler for CustomServerError {
    fn handle(&self) -> cot::Result<Response> {
        Ok(Response::new_html(
            StatusCode::INTERNAL_SERVER_ERROR,
            Body::fixed(include_str!("500.html")),
        ))
    }
}

struct CustomNotFound;
impl ErrorPageHandler for CustomNotFound {
    fn handle(&self) -> cot::Result<Response> {
        Ok(Response::new_html(
            StatusCode::NOT_FOUND,
            Body::fixed(include_str!("404.html")),
        ))
    }
}

#[cot::main]
fn main() -> impl Project {
    HelloProject
}
