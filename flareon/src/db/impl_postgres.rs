use derive_more::Debug;
use flareon::db::{SqlxRowRef, SqlxValueRef};
use log::debug;
use sea_query::{PostgresQueryBuilder, SchemaStatementBuilder};
use sea_query_binder::{SqlxBinder, SqlxValues};
use sqlx::{Database, PgPool, Row};

use super::{Result, RowsNum, StatementResult};

#[derive(Debug)]
pub(super) struct DatabasePostgres {
    db_connection: PgPool,
}

impl DatabasePostgres {
    pub(super) async fn new(url: &str) -> Result<Self> {
        let db_connection = PgPool::connect(url).await?;

        Ok(Self { db_connection })
    }

    pub(super) async fn close(&self) -> Result<()> {
        self.db_connection.close().await;
        Ok(())
    }

    pub(super) async fn fetch_option<T: SqlxBinder>(
        &self,
        statement: &T,
    ) -> Result<Option<PostgresRow>> {
        let (sql, values) = Self::build_sql(statement);

        let row = Self::sqlx_query_with(&sql, values)
            .fetch_optional(&self.db_connection)
            .await?;
        Ok(row.map(PostgresRow::new))
    }

    pub(super) async fn fetch_all<T: SqlxBinder>(&self, statement: &T) -> Result<Vec<PostgresRow>> {
        let (sql, values) = Self::build_sql(statement);

        let result = Self::sqlx_query_with(&sql, values)
            .fetch_all(&self.db_connection)
            .await?
            .into_iter()
            .map(PostgresRow::new)
            .collect();
        Ok(result)
    }

    pub(super) async fn execute_statement<T: SqlxBinder>(
        &self,
        statement: &T,
    ) -> Result<StatementResult> {
        let (sql, mut values) = Self::build_sql(statement);
        Self::prepare_values(&mut values);

        debug!("Postgres Query: `{}` (values: {:?})", sql, values);

        self.execute_sqlx(Self::sqlx_query_with(&sql, values)).await
    }

    pub(super) async fn execute_schema<T: SchemaStatementBuilder>(
        &self,
        statement: T,
    ) -> Result<StatementResult> {
        let sql = statement.build(PostgresQueryBuilder);
        debug!("Schema modification: {}", sql);

        self.execute_sqlx(sqlx::query(&sql)).await
    }

    pub(super) async fn raw_with(&self, sql: &str, values: SqlxValues) -> Result<StatementResult> {
        self.execute_sqlx(Self::sqlx_query_with(sql, values)).await
    }

    async fn execute_sqlx<'a, A>(
        &self,
        sqlx_statement: sqlx::query::Query<'a, sqlx::postgres::Postgres, A>,
    ) -> Result<StatementResult>
    where
        A: 'a + sqlx::IntoArguments<'a, sqlx::postgres::Postgres>,
    {
        let result = sqlx_statement.execute(&self.db_connection).await?;
        let result = StatementResult {
            rows_affected: RowsNum(result.rows_affected()),
        };

        debug!("Rows affected: {}", result.rows_affected.0);
        Ok(result)
    }

    fn build_sql<T>(statement: &T) -> (String, SqlxValues)
    where
        T: SqlxBinder,
    {
        let (sql, values) = statement.build_sqlx(PostgresQueryBuilder);
        debug!("Postgres Query: `{}` (values: {:?})", sql, values);

        (sql, values)
    }

    fn sqlx_query_with(
        sql: &str,
        mut values: SqlxValues,
    ) -> sqlx::query::Query<'_, sqlx::postgres::Postgres, SqlxValues> {
        Self::prepare_values(&mut values);
        sqlx::query_with(sql, values)
    }

    fn prepare_values(values: &mut SqlxValues) {
        for value in &mut values.0 .0 {
            Self::tinyint_to_smallint(value);
            Self::unsigned_to_signed(value);
        }
    }

    /// PostgreSQL does only support 2+ bytes integers, so we need to convert
    /// i8/u8 to i16/u16. Otherwise, sqlx will convert them internally to `char`
    /// and we'll get an error.
    fn tinyint_to_smallint(value: &mut sea_query::Value) {
        if let sea_query::Value::TinyInt(num) = value {
            *value = sea_query::Value::SmallInt(num.map(|v| v as i16));
        } else if let sea_query::Value::TinyUnsigned(num) = value {
            *value = sea_query::Value::SmallInt(num.map(|v| v as i16));
        }
    }

    /// PostgreSQL doesn't support unsigned integers, so we need to convert them
    /// to signed integers.
    fn unsigned_to_signed(value: &mut sea_query::Value) {
        if let sea_query::Value::SmallUnsigned(num) = value {
            *value = sea_query::Value::SmallInt(num.map(|v| v as i16));
        } else if let sea_query::Value::Unsigned(num) = value {
            *value = sea_query::Value::Int(num.map(|v| v as i32));
        } else if let sea_query::Value::BigUnsigned(num) = value {
            *value = sea_query::Value::BigInt(num.map(|v| v as i64));
        }
    }
}

#[derive(Debug)]
pub struct PostgresRow {
    #[debug("...")]
    inner: sqlx::postgres::PgRow,
}

impl PostgresRow {
    #[must_use]
    fn new(inner: sqlx::postgres::PgRow) -> Self {
        Self { inner }
    }
}

impl SqlxRowRef for PostgresRow {
    type ValueRef<'r> = PostgresValueRef<'r>;

    fn get_raw(&self, index: usize) -> Result<Self::ValueRef<'_>> {
        Ok(PostgresValueRef::new(self.inner.try_get_raw(index)?))
    }
}

#[derive(Debug)]
pub struct PostgresValueRef<'r> {
    #[debug("...")]
    inner: sqlx::postgres::PgValueRef<'r>,
}

impl<'r> PostgresValueRef<'r> {
    #[must_use]
    fn new(inner: sqlx::postgres::PgValueRef<'r>) -> Self {
        Self { inner }
    }
}

impl<'r> SqlxValueRef<'r> for PostgresValueRef<'r> {
    type DB = sqlx::Postgres;

    fn get_raw(self) -> <Self::DB as Database>::ValueRef<'r> {
        self.inner
    }
}
