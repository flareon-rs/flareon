use flareon::request::Request;
use flareon::response::{Response, ResponseExt};
use flareon::router::{Route, Router};
use flareon::test::Client;
use flareon::{Body, FlareonApp, FlareonProject, StatusCode};

async fn hello(_request: Request) -> flareon::Result<Response> {
    Ok(Response::new_html(StatusCode::OK, Body::fixed("OK")))
}

#[tokio::test]
async fn flareon_project_router_sub_path() {
    struct App1;
    impl FlareonApp for App1 {
        fn name(&self) -> &'static str {
            "app1"
        }

        fn router(&self) -> Router {
            Router::with_urls([Route::with_handler_and_name("/index", hello, "index")])
        }
    }

    struct App2;
    impl FlareonApp for App2 {
        fn name(&self) -> &'static str {
            "app2"
        }

        fn router(&self) -> Router {
            Router::with_urls([Route::with_handler_and_name("/hello", hello, "index")])
        }
    }

    let project = FlareonProject::builder()
        .register_app_with_views(App1, "/")
        .register_app_with_views(App2, "/app")
        .build()
        .await
        .unwrap();

    let mut client = Client::new(project);

    let response = client.get("/app/hello").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
