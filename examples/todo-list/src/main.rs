use std::sync::Arc;

use askama::Template;
use flareon::forms::Form;
use flareon::prelude::{Body, Error, FlareonApp, FlareonProject, Response, Route, StatusCode};
use flareon::request::Request;
use flareon::reverse;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
struct TodoItem {
    title: String,
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    request: &'a Request,
    todo_items: Vec<TodoItem>,
}

static TODOS: RwLock<Vec<TodoItem>> = RwLock::const_new(Vec::new());

async fn index(request: Request) -> Result<Response, Error> {
    let todo_items = (*TODOS.read().await).clone();
    let index_template = IndexTemplate {
        request: &request,
        todo_items,
    };
    let rendered = index_template.render().unwrap();

    Ok(Response::new_html(
        StatusCode::OK,
        Body::fixed(rendered.as_bytes().to_vec()),
    ))
}

#[derive(Debug, Form)]
struct TodoForm {
    #[form(opt(max_length = 100))]
    title: String,
}

async fn add_todo(mut request: Request) -> Result<Response, Error> {
    let todo_form = TodoForm::from_request(&mut request).await.unwrap();

    {
        let mut todos = TODOS.write().await;
        todos.push(TodoItem {
            title: todo_form.title,
        });
    }

    Ok(reverse!(request, "index"))
}

async fn remove_todo(request: Request) -> Result<Response, Error> {
    let todo_id = request.path_param("todo_id").expect("todo_id not found");
    let todo_id = todo_id.parse::<usize>().expect("todo_id is not a number");

    {
        let mut todos = TODOS.write().await;
        todos.remove(todo_id);
    }

    Ok(reverse!(request, "index"))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let todo_app = FlareonApp::builder()
        .urls([
            Route::with_handler_and_name("/", Arc::new(Box::new(index)), "index"),
            Route::with_handler_and_name("/todos/add", Arc::new(Box::new(add_todo)), "add-todo"),
            Route::with_handler_and_name(
                "/todos/:todo_id/remove",
                Arc::new(Box::new(remove_todo)),
                "remove-todo",
            ),
        ])
        .build()
        .unwrap();

    let todo_project = FlareonProject::builder()
        .register_app_with_views(todo_app, "")
        .build()
        .unwrap();

    flareon::run(todo_project, "127.0.0.1:8000").await.unwrap();
}
