use derive_more::Debug;
use flareon::db::{SqlxRowRef, SqlxValueRef};
use log::debug;
use sea_query::{QueryStatementWriter, SchemaStatementBuilder, SqliteQueryBuilder};
use sea_query_binder::{SqlxBinder, SqlxValues};
use sqlx::{Database, Row, SqlitePool};

use super::{Result, RowsNum, StatementResult};

#[derive(Debug)]
pub(super) struct DatabaseSqlite {
    db_connection: SqlitePool,
}

impl DatabaseSqlite {
    pub(super) async fn new(url: &str) -> Result<Self> {
        let db_connection = SqlitePool::connect(url).await?;

        Ok(Self { db_connection })
    }

    pub(super) async fn close(self) -> Result<()> {
        self.db_connection.close().await;
        Ok(())
    }

    pub(super) async fn fetch_one<T: SqlxBinder>(&self, statement: T) -> Result<SqliteRow> {
        let (sql, values) = Self::build_sql(statement);

        let row = sqlx::query_with(&sql, values)
            .fetch_one(&self.db_connection)
            .await?;
        Ok(SqliteRow::new(row))
    }

    pub(super) async fn fetch_all<T: SqlxBinder>(&self, statement: T) -> Result<Vec<SqliteRow>> {
        let (sql, values) = Self::build_sql(statement);

        let result = sqlx::query_with(&sql, values)
            .fetch_all(&self.db_connection)
            .await?
            .into_iter()
            .map(SqliteRow::new)
            .collect();
        Ok(result)
    }

    pub(super) async fn execute_statement<T: SqlxBinder>(
        &self,
        statement: T,
    ) -> Result<StatementResult> {
        let (sql, values) = Self::build_sql(statement);

        self.execute_sqlx(sqlx::query_with(&sql, values)).await
    }

    pub(super) async fn execute_schema<T: SchemaStatementBuilder>(
        &self,
        statement: T,
    ) -> Result<StatementResult> {
        let sql = statement.build(SqliteQueryBuilder);
        debug!("Schema modification: {}", sql);

        self.execute_sqlx(sqlx::query(&sql)).await
    }

    async fn execute_sqlx<'a, A>(
        &self,
        sqlx_statement: sqlx::query::Query<'a, sqlx::sqlite::Sqlite, A>,
    ) -> Result<StatementResult>
    where
        A: 'a + sqlx::IntoArguments<'a, sqlx::sqlite::Sqlite>,
    {
        let result = sqlx_statement.execute(&self.db_connection).await?;
        let result = StatementResult {
            rows_affected: RowsNum(result.rows_affected()),
        };

        debug!("Rows affected: {}", result.rows_affected.0);
        Ok(result)
    }

    fn build_sql<T>(statement: T) -> (String, SqlxValues)
    where
        T: SqlxBinder,
    {
        let (sql, values) = statement.build_sqlx(SqliteQueryBuilder);
        debug!("SQLite Query: {}", sql);

        (sql, values)
    }
}

#[derive(Debug)]
pub struct SqliteRow {
    #[debug("...")]
    inner: sqlx::sqlite::SqliteRow,
}

impl SqliteRow {
    #[must_use]
    fn new(inner: sqlx::sqlite::SqliteRow) -> Self {
        Self { inner }
    }
}

impl SqlxRowRef for SqliteRow {
    type ValueRef<'r> = SqliteValueRef<'r>;

    fn get_raw(&self, index: usize) -> Result<Self::ValueRef<'_>> {
        Ok(SqliteValueRef::new(self.inner.try_get_raw(index)?))
    }
}

#[derive(Debug)]
pub struct SqliteValueRef<'r> {
    #[debug("...")]
    inner: sqlx::sqlite::SqliteValueRef<'r>,
}

impl<'r> SqliteValueRef<'r> {
    #[must_use]
    fn new(inner: sqlx::sqlite::SqliteValueRef<'r>) -> Self {
        Self { inner }
    }
}

impl<'r> SqlxValueRef<'r> for SqliteValueRef<'r> {
    type DB = sqlx::Sqlite;

    fn get_raw(self) -> <Self::DB as Database>::ValueRef<'r> {
        self.inner
    }
}
