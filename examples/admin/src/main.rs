use cot::__private::async_trait;
use cot::admin::AdminApp;
use cot::auth::db::{DatabaseUser, DatabaseUserApp};
use cot::config::{DatabaseConfig, ProjectConfig};
use cot::middleware::SessionMiddleware;
use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::static_files::StaticFilesMiddleware;
use cot::{AppContext, Body, CotApp, CotProject, StatusCode};

async fn hello(_request: Request) -> cot::Result<Response> {
    Ok(Response::new_html(StatusCode::OK, Body::fixed("xd")))
}

struct HelloApp;

#[async_trait]
impl CotApp for HelloApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    async fn init(&self, context: &mut AppContext) -> cot::Result<()> {
        // TODO use transaction
        let user = DatabaseUser::get_by_username(context.database(), "admin").await?;
        if user.is_none() {
            DatabaseUser::create_user(context.database(), "admin", "admin").await?;
        }

        Ok(())
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", hello)])
    }
}

#[cot::main]
async fn main() -> cot::Result<CotProject> {
    let cot_project = CotProject::builder()
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
        .middleware_with_context(StaticFilesMiddleware::from_app_context)
        .middleware(SessionMiddleware::new())
        .build()
        .await?;

    Ok(cot_project)
}
