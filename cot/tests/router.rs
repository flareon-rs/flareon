use bytes::Bytes;
use cot::request::{Request, RequestExt};
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::test::Client;
use cot::{Body, CotApp, CotProject, StatusCode};

async fn index(_request: Request) -> cot::Result<Response> {
    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("Hello world!"),
    ))
}

async fn parameterized(request: Request) -> cot::Result<Response> {
    let name = request.path_params().get("name").unwrap().to_owned();

    Ok(Response::new_html(StatusCode::OK, Body::fixed(name)))
}

#[tokio::test]
#[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
async fn test_index() {
    let mut client = Client::new(project().await);

    let response = client.get("/").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        Bytes::from("Hello world!")
    );
}

#[tokio::test]
#[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
async fn path_params() {
    let mut client = Client::new(project().await);

    let response = client.get("/get/John").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        Bytes::from("John")
    );
}

#[must_use]
async fn project() -> CotProject {
    struct RouterApp;
    impl CotApp for RouterApp {
        fn name(&self) -> &'static str {
            "router-app"
        }

        fn router(&self) -> Router {
            Router::with_urls([
                Route::with_handler_and_name("/", index, "index"),
                Route::with_handler_and_name("/get/:name", parameterized, "parameterized"),
            ])
        }
    }

    CotProject::builder()
        .register_app_with_views(RouterApp, "")
        .build()
        .await
        .unwrap()
}
