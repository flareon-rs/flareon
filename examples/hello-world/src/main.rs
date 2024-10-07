use flareon::request::Request;
use flareon::response::{Response, ResponseExt};
use flareon::router::{Route, Router};
use flareon::{Body, FlareonApp, FlareonProject, StatusCode};

async fn return_hello(_request: Request) -> flareon::Result<Response> {
    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("<h1>Hello Flareon!</h1>".as_bytes().to_vec()),
    ))
}

struct HelloApp;

impl FlareonApp for HelloApp {
    fn name(&self) -> &str {
        env!("CARGO_PKG_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", return_hello)])
    }
}

#[tokio::main]
async fn main() {
    let flareon_project = FlareonProject::builder()
        .register_app_with_views(HelloApp, "")
        .build()
        .await
        .unwrap();

    flareon::run(flareon_project, "127.0.0.1:8000")
        .await
        .unwrap();
}
