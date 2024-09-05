mod migrations;

use std::sync::Arc;

use askama::Template;
use flareon::db::migrations::MigrationEngine;
use flareon::db::{model, query, Database, Model};
use flareon::forms::Form;
use flareon::request::Request;
use flareon::router::Route;
use flareon::{reverse, Body, Error, FlareonApp, FlareonProject, Response, StatusCode};
use tokio::sync::OnceCell;

#[derive(Debug, Clone)]
#[model]
struct TodoItem {
    id: i32,
    title: String,
}

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    request: &'a Request,
    todo_items: Vec<TodoItem>,
}

static DB: OnceCell<Database> = OnceCell::const_new();

async fn index(request: Request) -> Result<Response, Error> {
    let db = DB.get().unwrap();

    let todo_items = TodoItem::objects().all(db).await?;
    let index_template = IndexTemplate {
        request: &request,
        todo_items,
    };
    let rendered = index_template.render()?;

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
        let db = DB.get().unwrap();
        TodoItem {
            id: 0,
            title: todo_form.title,
        }
        .save(db)
        .await?;
    }

    Ok(reverse!(request, "index"))
}

async fn remove_todo(request: Request) -> Result<Response, Error> {
    let todo_id = request.path_param("todo_id").expect("todo_id not found");
    let todo_id = todo_id.parse::<i32>().expect("todo_id is not a number");

    {
        let db = DB.get().unwrap();
        query!(TodoItem, $id == todo_id).delete(db).await?;
    }

    Ok(reverse!(request, "index"))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let db = DB
        .get_or_init(|| async { Database::new("sqlite::memory:").await.unwrap() })
        .await;
    MigrationEngine::new(migrations::MIGRATIONS)
        .run(db)
        .await
        .unwrap();

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
        .build();

    flareon::run(todo_project, "127.0.0.1:8080").await.unwrap();
}
