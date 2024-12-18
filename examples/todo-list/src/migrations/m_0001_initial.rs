//! Generated by flareon CLI 0.1.0 on 2024-08-28 13:39:05+00:00

#[derive(Clone)]
pub(super) struct Migration;
impl ::flareon::db::migrations::Migration for Migration {
    const APP_NAME: &'static str = "example-todo-list";
    const MIGRATION_NAME: &'static str = "m_0001_initial";
    const DEPENDENCIES: &'static [::flareon::db::migrations::MigrationDependency] = &[];
    const OPERATIONS: &'static [::flareon::db::migrations::Operation] =
        &[::flareon::db::migrations::Operation::create_model()
            .table_name(::flareon::db::Identifier::new("todo_item"))
            .fields(&[
                ::flareon::db::migrations::Field::new(
                    ::flareon::db::Identifier::new("id"),
                    <i32 as ::flareon::db::DatabaseField>::TYPE,
                )
                .auto()
                .primary_key(),
                ::flareon::db::migrations::Field::new(
                    ::flareon::db::Identifier::new("title"),
                    <String as ::flareon::db::DatabaseField>::TYPE,
                ),
            ])
            .build()];
}

#[derive(::core::fmt::Debug)]
#[::flareon::db::model(model_type = "migration")]
struct _TodoItem {
    id: i32,
    title: String,
}
