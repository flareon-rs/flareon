use cot::__private::async_trait;
use cot::admin::AdminApp;
use cot::auth::db::{DatabaseUser, DatabaseUserApp};
use cot::cli::CliMetadata;
use cot::config::{DatabaseConfig, ProjectConfig};
use cot::middleware::SessionMiddleware;
use cot::project::{WithApps, WithConfig};
use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::static_files::StaticFilesMiddleware;
use cot::{App, AppBuilder, Body, BoxedHandler, Project, ProjectContext, StatusCode};
use rinja::Template;

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    request: &'a Request,
}

async fn index(request: Request) -> cot::Result<Response> {
    let index_template = IndexTemplate { request: &request };
    let rendered = index_template.render()?;

    Ok(Response::new_html(StatusCode::OK, Body::fixed(rendered)))
}

struct HelloApp;

#[async_trait]
impl App for HelloApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    async fn init(&self, context: &mut ProjectContext) -> cot::Result<()> {
        // TODO use transaction
        let user = DatabaseUser::get_by_username(context.database(), "admin").await?;
        if user.is_none() {
            DatabaseUser::create_user(context.database(), "admin", "admin").await?;
        }

        Ok(())
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", index)])
    }
}

struct AdminProject;

impl Project for AdminProject {
    fn cli_metadata(&self) -> CliMetadata {
        cot::cli::metadata!()
    }

    fn config(&self, _config_name: &str) -> cot::Result<ProjectConfig> {
        Ok(ProjectConfig::builder()
            .debug(true)
            .database(
                DatabaseConfig::builder()
                    .url("sqlite://db.sqlite3?mode=rwc")
                    .build(),
            )
            .build())
    }

    fn register_apps(&self, modules: &mut AppBuilder, _app_context: &ProjectContext<WithConfig>) {
        modules.register(DatabaseUserApp::new());
        modules.register_with_views(AdminApp::new(), "/admin");
        modules.register_with_views(HelloApp, "");
    }

    fn middlewares(
        &self,
        handler: cot::project::RootHandlerBuilder,
        app_context: &ProjectContext<WithApps>,
    ) -> BoxedHandler {
        handler
            .middleware(StaticFilesMiddleware::from_app_context(app_context))
            .middleware(SessionMiddleware::new())
            .build()
    }
}

#[cot::main]
fn main() -> impl Project {
    AdminProject
}
