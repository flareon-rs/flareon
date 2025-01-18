use cot::bytes::Bytes;
use cot::config::ProjectConfig;
use cot::middleware::LiveReloadMiddleware;
use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::static_files::StaticFilesMiddleware;
use cot::{static_files, Body, CotApp, CotProject, StatusCode};
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

impl CotApp for {{ app_name }} {
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

#[cot::main]
async fn main() -> cot::Result<CotProject> {
    let project = CotProject::builder()
        .config(ProjectConfig::builder().build())
        .with_cli(cot::cli::metadata!())
        .register_app_with_views({{ app_name }}, "")
        .middleware_with_context(StaticFilesMiddleware::from_app_context)
        .middleware(LiveReloadMiddleware::new())
        .build()
        .await?;

    Ok(project)
}
