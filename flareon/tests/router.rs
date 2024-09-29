use bytes::Bytes;
use flareon::request::{Request, RequestExt};
use flareon::response::{Response, ResponseExt};
use flareon::router::{Route, RouterService};
use flareon::test::Client;
use flareon::{Body, FlareonApp, FlareonProject, StatusCode};

async fn index(_request: Request) -> flareon::Result<Response> {
    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("Hello world!"),
    ))
}

async fn parameterized(request: Request) -> flareon::Result<Response> {
    let name = request.path_params().get("name").unwrap().to_owned();

    Ok(Response::new_html(StatusCode::OK, Body::fixed(name)))
}

#[tokio::test]
async fn test_index() {
    let mut client = Client::new(project());

    let response = client.get("/").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        Bytes::from("Hello world!")
    );
}

#[tokio::test]
async fn test_path_params() {
    let mut client = Client::new(project());

    let response = client.get("/get/John").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        Bytes::from("John")
    );
}

#[must_use]
fn project() -> FlareonProject<RouterService> {
    let app = FlareonApp::builder()
        .urls([
            Route::with_handler_and_name("/", index, "index"),
            Route::with_handler_and_name("/get/:name", parameterized, "parameterized"),
        ])
        .build()
        .unwrap();

    FlareonProject::builder()
        .register_app_with_views(app, "")
        .build()
}
