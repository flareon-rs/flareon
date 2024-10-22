mod migrations;

use askama::Template;
use flareon::db::migrations::MigrationEngine;
use flareon::db::{model, query, Database, Model};
use flareon::forms::Form;
use flareon::request::{Request, RequestExt};
use flareon::response::{Response, ResponseExt};
use flareon::router::{Route, Router};
use flareon::{reverse, Body, FlareonApp, FlareonProject, StatusCode};
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

async fn index(request: Request) -> flareon::Result<Response> {
    let db = DB.get().unwrap();

    let todo_items = TodoItem::objects().all(db).await?;
    let index_template = IndexTemplate {
        request: &request,
        todo_items,
    };
    let rendered = index_template.render()?;

    Ok(Response::new_html(StatusCode::OK, Body::fixed(rendered)))
}

#[derive(Debug, Form)]
struct TodoForm {
    #[form(opt(max_length = 100))]
    title: String,
}

async fn add_todo(mut request: Request) -> flareon::Result<Response> {
    let todo_form = TodoForm::from_request(&mut request).await?.unwrap();

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

async fn remove_todo(request: Request) -> flareon::Result<Response> {
    let todo_id = request
        .path_params()
        .get("todo_id")
        .expect("todo_id not found");
    let todo_id = todo_id.parse::<i32>().expect("todo_id is not a number");

    {
        let db = DB.get().unwrap();
        query!(TodoItem, $id == todo_id).delete(db).await?;
    }

    Ok(reverse!(request, "index"))
}

struct TodoApp;

impl FlareonApp for TodoApp {
    fn name(&self) -> &'static str {
        "todo-app"
    }

    fn router(&self) -> Router {
        Router::with_urls([
            Route::with_handler_and_name("/", index, "index"),
            Route::with_handler_and_name("/todos/add", add_todo, "add-todo"),
            Route::with_handler_and_name("/todos/:todo_id/remove", remove_todo, "remove-todo"),
        ])
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let db = DB
        .get_or_init(|| async { Database::new("sqlite::memory:").await.unwrap() })
        .await;
    MigrationEngine::new(migrations::MIGRATIONS.iter().copied())
        .run(db)
        .await
        .unwrap();

    let todo_project = FlareonProject::builder()
        .register_app_with_views(TodoApp, "")
        .build()
        .await
        .unwrap();

    flareon::run(todo_project, "127.0.0.1:8080").await.unwrap();
}
