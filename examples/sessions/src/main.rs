use askama::Template;
use flareon::forms::Form;
use flareon::middleware::SessionMiddleware;
use flareon::request::{Request, RequestExt};
use flareon::response::{Response, ResponseExt};
use flareon::router::{Route, Router};
use flareon::{reverse, Body, FlareonApp, FlareonProject, StatusCode};

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

async fn hello(request: Request) -> flareon::Result<Response> {
    let name: String = request
        .session()
        .get("user_name")
        .await
        .expect("Invalid session value")
        .unwrap_or_default();
    if name.is_empty() {
        return Ok(reverse!(request, "name"));
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

async fn name(mut request: Request) -> flareon::Result<Response> {
    if request.method() == flareon::Method::POST {
        let name_form = NameForm::from_request(&mut request).await?.unwrap();
        request
            .session_mut()
            .insert("user_name", name_form.name)
            .await
            .unwrap();

        return Ok(reverse!(request, "index"));
    }

    let template = NameTemplate { request: &request };

    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed(template.render()?),
    ))
}

struct HelloApp;

impl FlareonApp for HelloApp {
    fn name(&self) -> &str {
        env!("CARGO_PKG_NAME")
    }

    fn router(&self) -> Router {
        Router::with_urls([
            Route::with_handler_and_name("/", hello, "index"),
            Route::with_handler_and_name("/name", name, "name"),
        ])
    }
}

#[tokio::main]
async fn main() {
    let flareon_project = FlareonProject::builder()
        .register_app_with_views(HelloApp, "")
        .middleware(SessionMiddleware::new())
        .build()
        .await
        .unwrap();

    flareon::run(flareon_project, "127.0.0.1:8000")
        .await
        .unwrap();
}
