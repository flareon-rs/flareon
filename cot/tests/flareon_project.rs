use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::test::Client;
use cot::{Body, CotApp, CotProject, StatusCode};

async fn hello(_request: Request) -> cot::Result<Response> {
    Ok(Response::new_html(StatusCode::OK, Body::fixed("OK")))
}

#[tokio::test]
#[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
async fn cot_project_router_sub_path() {
    struct App1;
    impl CotApp for App1 {
        fn name(&self) -> &'static str {
            "app1"
        }

        fn router(&self) -> Router {
            Router::with_urls([Route::with_handler_and_name("/index", hello, "index")])
        }
    }

    struct App2;
    impl CotApp for App2 {
        fn name(&self) -> &'static str {
            "app2"
        }

        fn router(&self) -> Router {
            Router::with_urls([Route::with_handler_and_name("/hello", hello, "index")])
        }
    }

    let project = CotProject::builder()
        .register_app_with_views(App1, "/")
        .register_app_with_views(App2, "/app")
        .build()
        .await
        .unwrap();

    let mut client = Client::new(project);

    let response = client.get("/app/hello").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
