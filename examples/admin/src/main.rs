use flareon::__private::async_trait;
use flareon::admin::AdminApp;
use flareon::auth::db::{DatabaseUser, DatabaseUserApp};
use flareon::config::{DatabaseConfig, ProjectConfig};
use flareon::middleware::SessionMiddleware;
use flareon::request::Request;
use flareon::response::{Response, ResponseExt};
use flareon::router::{Route, Router};
use flareon::{AppContext, Body, FlareonApp, FlareonProject, StatusCode};

async fn hello(_request: Request) -> flareon::Result<Response> {
    Ok(Response::new_html(StatusCode::OK, Body::fixed("xd")))
}

struct HelloApp;

#[async_trait]
impl FlareonApp for HelloApp {
    fn name(&self) -> &str {
        env!("CARGO_PKG_NAME")
    }

    async fn init(&self, context: &mut AppContext) -> flareon::Result<()> {
        DatabaseUser::create_user(context.database(), "admin", "admin").await?;
        Ok(())
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", hello)])
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let flareon_project = FlareonProject::builder()
        .config(
            ProjectConfig::builder()
                .database_config(
                    DatabaseConfig::builder()
                        .url("sqlite://db.sqlite3?mode=rwc")
                        .build()
                        .unwrap(),
                )
                .build(),
        )
        .register_app(DatabaseUserApp::new())
        .register_app_with_views(AdminApp::new(), "/admin")
        .register_app_with_views(HelloApp, "")
        .middleware(SessionMiddleware::new())
        .build()
        .await
        .unwrap();

    flareon::run(flareon_project, "127.0.0.1:8000")
        .await
        .unwrap();
}
