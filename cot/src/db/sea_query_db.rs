/// Implements the database backend for a specific engine using `SeaQuery`.
///
/// Note that this macro doesn't implement certain engine-specific methods, and
/// they need to be implemented in a separate `impl` block. These methods are:
/// * `prepare_values`
/// * `sea_query_column_type_for`
macro_rules! impl_sea_query_db_backend {
    ($db_name:ident : $sqlx_db_ty:ty, $pool_ty:ty, $row_name:ident, $value_ref_name:ident, $query_builder:expr) => {
        /// A wrapper over [`$sqlx_db_ty`] that serves an in internal implementation of
        /// `Database` using `SeaQuery`.
        #[derive(Debug)]
        pub(super) struct $db_name {
            db_connection: $pool_ty,
        }

        impl $db_name {
            pub(super) async fn new(url: &str) -> crate::db::Result<Self> {
                let db_connection = <$pool_ty>::connect(url).await?;

                let db = Self { db_connection };
                db.init().await?;
                Ok(db)
            }

            pub(super) async fn close(&self) -> crate::db::Result<()> {
                self.db_connection.close().await;
                Ok(())
            }

            pub(super) async fn fetch_option<T: sea_query_binder::SqlxBinder>(
                &self,
                statement: &T,
            ) -> crate::db::Result<Option<$row_name>> {
                let (sql, values) = Self::build_sql(statement);

                let row = Self::sqlx_query_with(&sql, values)
                    .fetch_optional(&self.db_connection)
                    .await?;
                Ok(row.map($row_name::new))
            }

            pub(super) async fn fetch_all<T: sea_query_binder::SqlxBinder>(
                &self,
                statement: &T,
            ) -> crate::db::Result<Vec<$row_name>> {
                let (sql, values) = Self::build_sql(statement);

                let result = Self::sqlx_query_with(&sql, values)
                    .fetch_all(&self.db_connection)
                    .await?
                    .into_iter()
                    .map($row_name::new)
                    .collect();
                Ok(result)
            }

            pub(super) async fn execute_statement<T: sea_query_binder::SqlxBinder>(
                &self,
                statement: &T,
            ) -> crate::db::Result<crate::db::StatementResult> {
                let (sql, mut values) = Self::build_sql(statement);
                Self::prepare_values(&mut values);

                self.execute_sqlx(Self::sqlx_query_with(&sql, values)).await
            }

            pub(super) async fn execute_schema<T: sea_query::SchemaStatementBuilder>(
                &self,
                statement: T,
            ) -> crate::db::Result<crate::db::StatementResult> {
                let sql = statement.build($query_builder);
                tracing::debug!("Schema modification: {}", sql);

                self.execute_sqlx(sqlx::query(&sql)).await
            }

            pub(super) async fn raw_with(
                &self,
                sql: &str,
                values: sea_query_binder::SqlxValues,
            ) -> crate::db::Result<crate::db::StatementResult> {
                self.execute_sqlx(Self::sqlx_query_with(sql, values)).await
            }

            async fn execute_sqlx<'a, A>(
                &self,
                sqlx_statement: sqlx::query::Query<'a, $sqlx_db_ty, A>,
            ) -> crate::db::Result<crate::db::StatementResult>
            where
                A: 'a + sqlx::IntoArguments<'a, $sqlx_db_ty>,
            {
                let result = sqlx_statement.execute(&self.db_connection).await?;
                let result = crate::db::StatementResult {
                    rows_affected: crate::db::RowsNum(result.rows_affected()),
                    last_inserted_row_id: Self::last_inserted_row_id_for(&result),
                };

                tracing::debug!("Rows affected: {}", result.rows_affected.0);
                Ok(result)
            }

            fn build_sql<T>(statement: &T) -> (String, sea_query_binder::SqlxValues)
            where
                T: sea_query_binder::SqlxBinder,
            {
                let (sql, values) = statement.build_sqlx($query_builder);

                (sql, values)
            }

            fn sqlx_query_with(
                sql: &str,
                mut values: sea_query_binder::SqlxValues,
            ) -> sqlx::query::Query<'_, $sqlx_db_ty, sea_query_binder::SqlxValues> {
                Self::prepare_values(&mut values);
                tracing::debug!("Query: `{}` (values: {:?})", sql, values);

                sqlx::query_with(sql, values)
            }
        }

        #[doc = "A wrapper for the internal row type used by [`"]
        #[doc = stringify!($sqlx_db_ty)]
        #[doc = "`] to provide a unified interface for the database operations."]
        #[derive(derive_more::Debug)]
        pub struct $row_name {
            #[debug("...")]
            inner: <$sqlx_db_ty as sqlx::Database>::Row,
        }

        impl $row_name {
            #[must_use]
            fn new(inner: <$sqlx_db_ty as sqlx::Database>::Row) -> Self {
                Self { inner }
            }
        }

        impl crate::db::SqlxRowRef for $row_name {
            type ValueRef<'r> = $value_ref_name<'r>;

            fn get_raw(&self, index: usize) -> crate::db::Result<Self::ValueRef<'_>> {
                use sqlx::Row;
                Ok($value_ref_name::new(self.inner.try_get_raw(index)?))
            }
        }

        #[doc = "A wrapper for the internal value type used by [`"]
        #[doc = stringify!($sqlx_db_ty)]
        #[doc = "`] to provide a unified interface for the database operations."]
        #[derive(derive_more::Debug)]
        pub struct $value_ref_name<'r> {
            #[debug("...")]
            inner: <$sqlx_db_ty as sqlx::Database>::ValueRef<'r>,
        }

        impl<'r> $value_ref_name<'r> {
            #[must_use]
            fn new(inner: <$sqlx_db_ty as sqlx::Database>::ValueRef<'r>) -> Self {
                Self { inner }
            }
        }

        impl<'r> crate::db::SqlxValueRef<'r> for $value_ref_name<'r> {
            type DB = $sqlx_db_ty;

            fn get_raw(self) -> <Self::DB as sqlx::Database>::ValueRef<'r> {
                self.inner
            }
        }
    };
}

pub(super) use impl_sea_query_db_backend;
