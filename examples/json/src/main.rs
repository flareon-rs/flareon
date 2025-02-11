use cot::cli::CliMetadata;
use cot::config::ProjectConfig;
use cot::project::WithConfig;
use cot::request::{Request, RequestExt};
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::{App, AppBuilder, Project, ProjectContext, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
struct AddRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Clone, Serialize)]
struct AddResponse {
    result: i32,
}

async fn add(mut request: Request) -> cot::Result<Response> {
    let add_request: AddRequest = request.json().await?;
    let response = AddResponse {
        result: add_request.a + add_request.b,
    };

    Response::new_json(StatusCode::OK, &response)
}

struct AddApp;

impl App for AddApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", add)])
    }
}

// Test with:
// curl --header "Content-Type: application/json" --request POST --data '{"a": 123, "b": 456}' 'http://127.0.0.1:8080/'

struct JsonProject;

impl Project for JsonProject {
    fn cli_metadata(&self) -> CliMetadata {
        cot::cli::metadata!()
    }

    fn config(&self, _config_name: &str) -> cot::Result<ProjectConfig> {
        Ok(ProjectConfig::dev_default())
    }

    fn register_apps(&self, modules: &mut AppBuilder, _app_context: &ProjectContext<WithConfig>) {
        modules.register_with_views(AddApp, "");
    }
}

#[cot::main]
fn main() -> impl Project {
    JsonProject
}
