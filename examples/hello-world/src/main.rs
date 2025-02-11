use cot::cli::CliMetadata;
use cot::config::ProjectConfig;
use cot::project::WithConfig;
use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::{App, AppBuilder, Body, Project, ProjectContext, StatusCode};

async fn return_hello(_request: Request) -> cot::Result<Response> {
    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("<h1>Hello Cot!</h1>".as_bytes().to_vec()),
    ))
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
        Ok(ProjectConfig::dev_default())
    }

    fn register_apps(&self, modules: &mut AppBuilder, _app_context: &ProjectContext<WithConfig>) {
        modules.register_with_views(HelloApp, "");
    }
}

#[cot::main]
fn main() -> impl Project {
    HelloProject
}
