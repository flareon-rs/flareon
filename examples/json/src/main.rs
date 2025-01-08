use flareon::request::{Request, RequestExt};
use flareon::response::{Response, ResponseExt};
use flareon::router::{Route, Router};
use flareon::{FlareonApp, FlareonProject, StatusCode};
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

async fn add(mut request: Request) -> flareon::Result<Response> {
    let add_request: AddRequest = request.json().await?;
    let response = AddResponse {
        result: add_request.a + add_request.b,
    };

    Response::new_json(StatusCode::OK, &response)
}

struct AddApp;

impl FlareonApp for AddApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", add)])
    }
}

// Test with:
// curl --header "Content-Type: application/json" --request POST --data '{"a": 123, "b": 456}' 'http://127.0.0.1:8080/'

#[flareon::main]
async fn main() -> flareon::Result<FlareonProject> {
    let flareon_project = FlareonProject::builder()
        .register_app_with_views(AddApp, "")
        .build()
        .await?;

    Ok(flareon_project)
}
