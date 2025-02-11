use cot::config::ProjectConfig;
use cot::project::WithConfig;
use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::test::Client;
use cot::{App, AppBuilder, Body, Project, ProjectContext, StatusCode};

async fn hello(_request: Request) -> cot::Result<Response> {
    Ok(Response::new_html(StatusCode::OK, Body::fixed("OK")))
}

#[tokio::test]
#[cfg_attr(miri, ignore)] // unsupported operation: can't call foreign function `sqlite3_open_v2`
async fn cot_project_router_sub_path() {
    struct App1;
    impl App for App1 {
        fn name(&self) -> &'static str {
            "app1"
        }

        fn router(&self) -> Router {
            Router::with_urls([Route::with_handler_and_name("/index", hello, "index")])
        }
    }

    struct App2;
    impl App for App2 {
        fn name(&self) -> &'static str {
            "app2"
        }

        fn router(&self) -> Router {
            Router::with_urls([Route::with_handler_and_name("/hello", hello, "index")])
        }
    }

    struct TestProject;
    impl Project for TestProject {
        fn config(&self, config_name: &str) -> cot::Result<ProjectConfig> {
            assert!(
                (config_name == "test"),
                "unexpected config name: {config_name}"
            );
            Ok(ProjectConfig::default())
        }

        fn register_apps(&self, apps: &mut AppBuilder, _context: &ProjectContext<WithConfig>) {
            apps.register_with_views(App1, "");
            apps.register_with_views(App2, "/app");
        }
    }

    let mut client = Client::new(TestProject).await;

    let response = client.get("/app/hello").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
