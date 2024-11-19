use crate::db::sea_query_db::impl_sea_query_db_backend;

impl_sea_query_db_backend!(DatabaseSqlite: sqlx::sqlite::Sqlite, sqlx::sqlite::SqlitePool, SqliteRow, SqliteValueRef, sea_query::SqliteQueryBuilder);

impl DatabaseSqlite {
    fn prepare_values(_values: &mut sea_query_binder::SqlxValues) {
        // No changes are needed for SQLite
    }

    pub(super) fn sea_query_column_type_for(
        &self,
        column_type: crate::db::ColumnType,
    ) -> sea_query::ColumnType {
        sea_query::ColumnType::from(column_type)
    }
}
