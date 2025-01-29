//! Database interface implementation – SQLite backend.

use sea_query_binder::SqlxValues;

use crate::db::sea_query_db::impl_sea_query_db_backend;

impl_sea_query_db_backend!(DatabaseSqlite: sqlx::sqlite::Sqlite, sqlx::sqlite::SqlitePool, SqliteRow, SqliteValueRef, sea_query::SqliteQueryBuilder);

impl DatabaseSqlite {
    async fn init(&self) -> crate::db::Result<()> {
        self.raw("PRAGMA foreign_keys = ON").await?;
        Ok(())
    }

    async fn raw(&self, sql: &str) -> crate::db::Result<crate::db::StatementResult> {
        self.raw_with(sql, SqlxValues(sea_query::Values(Vec::new())))
            .await
    }

    fn prepare_values(_values: &mut SqlxValues) {
        // No changes are needed for SQLite
    }

    #[allow(clippy::unnecessary_wraps)] // to have a unified interface between database impls
    fn last_inserted_row_id_for(result: &sqlx::sqlite::SqliteQueryResult) -> Option<u64> {
        #[allow(clippy::cast_sign_loss)]
        Some(result.last_insert_rowid() as u64)
    }

    #[allow(clippy::unused_self)] // to have a unified interface between database impls
    pub(super) fn sea_query_column_type_for(
        &self,
        column_type: crate::db::ColumnType,
    ) -> sea_query::ColumnType {
        sea_query::ColumnType::from(column_type)
    }
}
