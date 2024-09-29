use flareon::request::Request;
use flareon::response::{Response, ResponseExt};
use flareon::router::Route;
use flareon::{Body, FlareonApp, FlareonProject, StatusCode};

async fn return_hello(_request: Request) -> flareon::Result<Response> {
    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("<h1>Hello Flareon!</h1>".as_bytes().to_vec()),
    ))
}

#[tokio::main]
async fn main() {
    let hello_app = FlareonApp::builder()
        .urls([Route::with_handler("/", return_hello)])
        .build()
        .unwrap();

    let flareon_project = FlareonProject::builder()
        .register_app_with_views(hello_app, "")
        .build();

    flareon::run(flareon_project, "127.0.0.1:8000")
        .await
        .unwrap();
}
