use cot::form::Form;
use cot::middleware::SessionMiddleware;
use cot::request::{Request, RequestExt};
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::{reverse_redirect, Body, CotApp, CotProject, StatusCode};
use rinja::Template;

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    request: &'a Request,
    name: String,
}

#[derive(Debug, Template)]
#[template(path = "name.html")]
struct NameTemplate<'a> {
    request: &'a Request,
}

#[derive(Debug, Form)]
struct NameForm {
    #[form(opt(max_length = 100))]
    name: String,
}

async fn hello(request: Request) -> cot::Result<Response> {
    let name: String = request
        .session()
        .get("user_name")
        .await
        .expect("Invalid session value")
        .unwrap_or_default();
    if name.is_empty() {
        return Ok(reverse_redirect!(request, "name")?);
    }

    let template = IndexTemplate {
        request: &request,
        name,
    };

    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed(template.render()?),
    ))
}

async fn name(mut request: Request) -> cot::Result<Response> {
    if request.method() == cot::Method::POST {
        let name_form = NameForm::from_request(&mut request).await?.unwrap();
        request
            .session_mut()
            .insert("user_name", name_form.name)
            .await
            .unwrap();

        return Ok(reverse_redirect!(request, "index")?);
    }

    let template = NameTemplate { request: &request };

    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed(template.render()?),
    ))
}

struct HelloApp;

impl CotApp for HelloApp {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([
            Route::with_handler_and_name("/", hello, "index"),
            Route::with_handler_and_name("/name", name, "name"),
        ])
    }
}

#[cot::main]
async fn main() -> cot::Result<CotProject> {
    let cot_project = CotProject::builder()
        .with_cli(cot::cli::metadata!())
        .register_app_with_views(HelloApp, "")
        .middleware(SessionMiddleware::new())
        .build()
        .await?;

    Ok(cot_project)
}
