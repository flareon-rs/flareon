use std::sync::Arc;

use flareon::prelude::{Body, Error, FlareonApp, FlareonProject, Response, StatusCode};
use flareon::request::Request;
use flareon::router::Route;

async fn return_hello(_request: Request) -> Result<Response, Error> {
    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("<h1>Hello Flareon!</h1>".as_bytes().to_vec()),
    ))
}

#[tokio::main]
async fn main() {
    let hello_app = FlareonApp::builder()
        .urls([Route::with_handler("", Arc::new(Box::new(return_hello)))])
        .build()
        .unwrap();

    let flareon_project = FlareonProject::builder()
        .register_app_with_views(hello_app, "")
        .build()
        .unwrap();

    flareon::run(flareon_project, "127.0.0.1:8000")
        .await
        .unwrap();
}
