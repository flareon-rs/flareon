mod migrations;

use cot::config::{DatabaseConfig, ProjectConfig};
use cot::db::migrations::DynMigration;
use cot::db::{model, query, Model};
use cot::forms::Form;
use cot::request::{Request, RequestExt};
use cot::response::{Response, ResponseExt};
use cot::router::{Route, Router};
use cot::{reverse_redirect, Body, CotApp, CotProject, StatusCode};
use rinja::Template;

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

async fn index(request: Request) -> cot::Result<Response> {
    let todo_items = TodoItem::objects().all(request.db()).await?;
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

async fn add_todo(mut request: Request) -> cot::Result<Response> {
    let todo_form = TodoForm::from_request(&mut request).await?.unwrap();

    {
        TodoItem {
            id: 0,
            title: todo_form.title,
        }
        .save(request.db())
        .await?;
    }

    Ok(reverse_redirect!(request, "index"))
}

async fn remove_todo(request: Request) -> cot::Result<Response> {
    let todo_id = request
        .path_params()
        .get("todo_id")
        .expect("todo_id not found");
    let todo_id = todo_id.parse::<i32>().expect("todo_id is not a number");

    {
        query!(TodoItem, $id == todo_id)
            .delete(request.db())
            .await?;
    }

    Ok(reverse_redirect!(request, "index"))
}

struct TodoApp;

impl CotApp for TodoApp {
    fn name(&self) -> &'static str {
        "todo-app"
    }

    fn migrations(&self) -> Vec<Box<dyn DynMigration>> {
        // TODO: this is way too complicated for the user-facing API
        #[allow(trivial_casts)]
        migrations::MIGRATIONS
            .iter()
            .copied()
            .map(|x| Box::new(x) as Box<dyn DynMigration>)
            .collect()
    }

    fn router(&self) -> Router {
        Router::with_urls([
            Route::with_handler_and_name("/", index, "index"),
            Route::with_handler_and_name("/todos/add", add_todo, "add-todo"),
            Route::with_handler_and_name("/todos/:todo_id/remove", remove_todo, "remove-todo"),
        ])
    }
}

#[cot::main]
async fn main() -> cot::Result<CotProject> {
    let todo_project = CotProject::builder()
        .config(
            ProjectConfig::builder()
                .database_config(
                    DatabaseConfig::builder()
                        .url("sqlite::memory:")
                        .build()
                        .unwrap(),
                )
                .build(),
        )
        .register_app_with_views(TodoApp, "")
        .build()
        .await?;

    Ok(todo_project)
}
