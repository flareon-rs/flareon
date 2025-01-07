//! Generated by flareon CLI 0.1.0 on 2024-11-12 15:49:48+00:00

#[derive(Debug, Copy, Clone)]
pub(super) struct Migration;
impl ::flareon::db::migrations::Migration for Migration {
    const APP_NAME: &'static str = "flareon";
    const MIGRATION_NAME: &'static str = "m_0001_initial";
    const DEPENDENCIES: &'static [::flareon::db::migrations::MigrationDependency] = &[];
    const OPERATIONS: &'static [::flareon::db::migrations::Operation] = &[
        ::flareon::db::migrations::Operation::create_model()
            .table_name(::flareon::db::Identifier::new("database_user"))
            .fields(
                &[
                    ::flareon::db::migrations::Field::new(
                            ::flareon::db::Identifier::new("id"),
                            <i64 as ::flareon::db::DatabaseField>::TYPE,
                        )
                        .auto()
                        .primary_key()
                        .set_null(<i64 as ::flareon::db::DatabaseField>::NULLABLE),
                    ::flareon::db::migrations::Field::new(
                            ::flareon::db::Identifier::new("username"),
                            <crate::db::LimitedString<
                                { crate::auth::db::MAX_USERNAME_LENGTH },
                            > as ::flareon::db::DatabaseField>::TYPE,
                        )
                        .set_null(
                            <crate::db::LimitedString<
                                { crate::auth::db::MAX_USERNAME_LENGTH },
                            > as ::flareon::db::DatabaseField>::NULLABLE,
                        )
                        .unique(),
                    ::flareon::db::migrations::Field::new(
                            ::flareon::db::Identifier::new("password"),
                            <crate::auth::PasswordHash as ::flareon::db::DatabaseField>::TYPE,
                        )
                        .set_null(
                            <crate::auth::PasswordHash as ::flareon::db::DatabaseField>::NULLABLE,
                        ),
                ],
            )
            .build(),
    ];
}

#[derive(::core::fmt::Debug)]
#[::flareon::db::model(model_type = "migration")]
struct _DatabaseUser {
    id: i64,
    #[model(unique)]
    username: crate::db::LimitedString<{ crate::auth::db::MAX_USERNAME_LENGTH }>,
    password: crate::auth::PasswordHash,
}
