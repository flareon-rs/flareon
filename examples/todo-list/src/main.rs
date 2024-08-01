use std::sync::Arc;

use askama::Template;
use flareon::forms::{AsFormField, CharField, Form, FormField};
use flareon::prelude::{Body, Error, FlareonApp, FlareonProject, Response, Route, StatusCode};
use flareon::request::Request;

#[derive(Debug)]
struct TodoItem {
    id: u32,
    title: String,
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    todo_items: Vec<TodoItem>,
}

async fn index(_request: Request) -> Result<Response, Error> {
    let index_template = IndexTemplate {
        todo_items: vec![
            TodoItem {
                id: 1,
                title: "Buy milk".to_string(),
            },
            TodoItem {
                id: 2,
                title: "Buy eggs".to_string(),
            },
        ],
    };
    let rendered = index_template.render().unwrap();

    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed(rendered.as_bytes().to_vec()),
    ))
}

#[derive(Debug, Form)]
struct TodoForm {
    title: String,
}

async fn add_todo(mut request: Request) -> Result<Response, Error> {
    let todo_form = TodoForm::from_request(&mut request).await.unwrap();

    println!("todo_form: {:?}", todo_form);

    // TODO add to global list

    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed("redirect is not implemented yet".as_bytes().to_vec()),
    ))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let todo_app = FlareonApp::builder()
        .urls([
            Route::with_handler("/", Arc::new(Box::new(index))),
            Route::with_handler("/add", Arc::new(Box::new(add_todo))),
        ])
        .build()
        .unwrap();

    let todo_project = FlareonProject::builder()
        .register_app_with_views(todo_app, "")
        .build()
        .unwrap();

    flareon::run(todo_project, "127.0.0.1:8000").await.unwrap();
}
