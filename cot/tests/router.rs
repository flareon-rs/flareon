use bytes::Bytes;
use cot::config::ProjectConfig;
use cot::project::WithConfig;
use cot::request::{Request, RequestExt};
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::test::Client;
use cot::{App, AppBuilder, Body, Project, ProjectContext, StatusCode};

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
    let client = Client::new(project().await);

    let response = client.await.get("/").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        Bytes::from("Hello world!")
    );
}

#[tokio::test]
#[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
async fn path_params() {
    let client = Client::new(project().await);

    let response = client.await.get("/get/John").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        Bytes::from("John")
    );
}

#[must_use]
async fn project() -> impl Project {
    struct RouterApp;
    impl App for RouterApp {
        fn name(&self) -> &'static str {
            "router-app"
        }

        fn router(&self) -> Router {
            Router::with_urls([
                Route::with_handler_and_name("/", index, "index"),
                Route::with_handler_and_name("/get/{name}", parameterized, "parameterized"),
            ])
        }
    }

    struct TestProject;
    impl Project for TestProject {
        fn config(&self, _config_name: &str) -> cot::Result<ProjectConfig> {
            Ok(ProjectConfig::default())
        }

        fn register_apps(&self, apps: &mut AppBuilder, context: &ProjectContext<WithConfig>) {
            apps.register_with_views(RouterApp, "");
        }
    }

    TestProject
}
