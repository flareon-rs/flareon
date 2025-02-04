use cot::bytes::Bytes;
use cot::cli::CliMetadata;
use cot::middleware::LiveReloadMiddleware;
use cot::project::{RootHandlerBuilder, WithApps, WithConfig};
use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::static_files::StaticFilesMiddleware;
use cot::{static_files, App, AppBuilder, Body, BoxedHandler, Project, ProjectContext, StatusCode};
use rinja::Template;

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

async fn index(request: Request) -> cot::Result<Response> {
    let index_template = IndexTemplate {};
    let rendered = index_template.render()?;

    Ok(Response::new_html(StatusCode::OK, Body::fixed(rendered)))
}

struct {{ app_name }};

impl App for {{ app_name }} {
    fn name(&self) -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler_and_name("/", index, "index")])
    }

    fn static_files(&self) -> Vec<(String, Bytes)> {
        static_files!("css/main.css")
    }
}

struct {{ project_struct_name }};

impl Project for {{ project_struct_name }} {
    fn cli_metadata(&self) -> CliMetadata {
        cot::cli::metadata!()
    }

    fn register_apps(&self, modules: &mut AppBuilder, _app_context: &ProjectContext<WithConfig>) {
        modules.register_with_views({{ app_name }}, "");
    }

    fn middlewares(
        &self,
        handler: RootHandlerBuilder,
        context: &ProjectContext<WithApps>,
    ) -> BoxedHandler {
        handler
            .middleware(StaticFilesMiddleware::from_app_context(context))
            .middleware(LiveReloadMiddleware::from_app_context(context))
            .build()
    }
}

#[cot::main]
fn main() -> impl Project {
    {{ project_struct_name }}
}
