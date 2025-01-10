use cot::request::Request;
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::{Body, CotApp, CotProject, StatusCode};

async fn return_hello(_request: Request) -> cot::Result<Response> {
    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("<h1>Hello Cot!</h1>".as_bytes().to_vec()),
    ))
}

struct HelloApp;

impl CotApp for HelloApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([Route::with_handler("/", return_hello)])
    }
}

#[cot::main]
async fn main() -> cot::Result<CotProject> {
    let cot_project = CotProject::builder()
        .register_app_with_views(HelloApp, "")
        .build()
        .await?;

    Ok(cot_project)
}
