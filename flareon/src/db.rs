//! Database support along with migration engine and ORM.
//!
//! This module contains the database connection structure, the model trait, and
//! the error types that can occur when interacting with the database.

mod fields;
#[cfg(feature = "mysql")]
pub mod impl_mysql;
#[cfg(feature = "postgres")]
pub mod impl_postgres;
#[cfg(feature = "sqlite")]
pub mod impl_sqlite;
pub mod migrations;
pub mod query;
mod relations;
mod sea_query_db;

use std::fmt::Write;
use std::hash::Hash;

use async_trait::async_trait;
use derive_more::{Debug, Deref, Display};
pub use flareon_macros::{model, query};
#[cfg(test)]
use mockall::automock;
use query::Query;
pub use relations::{ForeignKey, ForeignKeyOnDeletePolicy, ForeignKeyOnUpdatePolicy};
use sea_query::{Iden, IntoColumnRef, ReturningClause, SchemaStatementBuilder, SimpleExpr};
use sea_query_binder::{SqlxBinder, SqlxValues};
use sqlx::{Type, TypeInfo};
use thiserror::Error;
use tracing::debug;

#[cfg(feature = "mysql")]
use crate::db::impl_mysql::{DatabaseMySql, MySqlRow, MySqlValueRef};
#[cfg(feature = "postgres")]
use crate::db::impl_postgres::{DatabasePostgres, PostgresRow, PostgresValueRef};
#[cfg(feature = "sqlite")]
use crate::db::impl_sqlite::{DatabaseSqlite, SqliteRow, SqliteValueRef};
use crate::db::migrations::ColumnTypeMapper;

/// An error that can occur when interacting with the database.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DatabaseError {
    /// Database engine error.
    #[error("Database engine error: {0}")]
    DatabaseEngineError(#[from] sqlx::Error),
    /// Error when building query.
    #[error("Error when building query: {0}")]
    QueryBuildingError(#[from] sea_query::error::Error),
    /// Type mismatch in database value.
    #[error("Type mismatch in database value: expected `{expected}`, found `{found}`. Perhaps migration is needed."
    )]
    TypeMismatch { expected: String, found: String },
    /// Error when decoding database value.
    #[error("Error when decoding database value: {0}")]
    ValueDecode(Box<dyn std::error::Error + 'static + Send + Sync>),
    /// Error when applying migrations.
    #[error("Error when applying migrations: {0}")]
    MigrationError(#[from] migrations::MigrationEngineError),
    /// Foreign Key could not be retrieved from the database because the record
    /// was not found.
    #[error("Error retrieving a Foreign Key from the database: record not found")]
    ForeignKeyNotFound,
}

impl DatabaseError {
    /// Creates a new database error from a value decode error.
    #[must_use]
    pub fn value_decode(error: impl std::error::Error + 'static + Send + Sync) -> Self {
        Self::ValueDecode(Box::new(error))
    }
}

/// An alias for [`Result`] that uses [`DatabaseError`] as the error type.
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// A model trait for database models.
///
/// This trait is used to define a model that can be stored in a database.
/// It is used to define the structure of the model, the table name, and the
/// columns.
///
/// # Deriving
///
/// This trait can, and should be derived using the [`model`] attribute macro.
/// This macro generates the implementation of the trait for the type, including
/// the implementation of the `Fields` helper struct.
///
/// ```rust
/// use flareon::db::model;
///
/// #[model]
/// struct MyModel {
///     id: i32,
///     name: String,
/// }
/// ```
#[async_trait]
pub trait Model: Sized + Send + 'static {
    /// A helper structure for the fields of the model.
    ///
    /// This structure should a constant [`FieldRef`](query::FieldRef) instance
    /// for each field in the model. Note that the names of the fields
    /// should be written in UPPER_SNAKE_CASE, just like other constants in
    /// Rust.
    type Fields;

    /// The primary key type of the model.
    type PrimaryKey: PrimaryKey;

    /// The name of the table in the database.
    const TABLE_NAME: Identifier;

    /// The name of the primary key column in the database.
    const PRIMARY_KEY_NAME: Identifier;

    /// The columns of the model.
    const COLUMNS: &'static [Column];

    /// Creates a model instance from a database row.
    ///
    /// # Errors
    ///
    /// This method can return an error if the data in the row is not compatible
    /// with the model.
    fn from_db(db_row: Row) -> Result<Self>;

    fn update_from_db(&mut self, db_row: Row, columns: &[usize]) -> Result<()>;

    /// Returns the primary key of the model.
    fn primary_key(&self) -> &Self::PrimaryKey;

    /// Used by the ORM to set the primary key of the model after it has been
    /// saved to the database.
    fn set_primary_key(&mut self, primary_key: Self::PrimaryKey);

    /// Gets the values of the model for the given columns.
    fn get_values(&self, columns: &[usize]) -> Vec<&dyn ToDbFieldValue>;

    /// Returns a query for all objects of this model.
    #[must_use]
    fn objects() -> Query<Self> {
        Query::new()
    }

    async fn get_by_primary_key<DB: DatabaseBackend>(
        db: &DB,
        pk: Self::PrimaryKey,
    ) -> Result<Option<Self>>;

    /// Saves the model to the database.
    ///
    /// # Errors
    ///
    /// This method can return an error if the model could not be saved to the
    /// database.
    async fn save<DB: DatabaseBackend>(&mut self, db: &DB) -> Result<()> {
        db.insert(self).await?;
        Ok(())
    }
}

/// An identifier structure that holds table or column name as a string.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Display, Deref)]
pub struct Identifier(&'static str);

impl Identifier {
    /// Creates a new identifier from a static string.
    #[must_use]
    pub const fn new(s: &'static str) -> Self {
        Self(s)
    }

    /// Returns the inner string of the identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0
    }
}

impl From<&'static str> for Identifier {
    fn from(s: &'static str) -> Self {
        Self::new(s)
    }
}

impl Iden for Identifier {
    fn unquoted(&self, s: &mut dyn Write) {
        s.write_str(self.as_str()).unwrap();
    }
}
impl Iden for &Identifier {
    fn unquoted(&self, s: &mut dyn Write) {
        s.write_str(self.as_str()).unwrap();
    }
}

/// A column structure that holds the name of the column and some additional
/// schema information.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Column {
    name: Identifier,
}

impl Column {
    /// Creates a new column with the given name.
    #[must_use]
    pub const fn new(name: Identifier) -> Self {
        Self { name }
    }
}

/// A marker trait that denotes that a type can be used as a primary key in a
/// database.
pub trait PrimaryKey: DatabaseField + Clone {}

/// A row structure that holds the data of a single row retrieved from the
/// database.
#[non_exhaustive]
#[derive(Debug)]
pub enum Row {
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteRow),
    #[cfg(feature = "postgres")]
    Postgres(PostgresRow),
    #[cfg(feature = "mysql")]
    MySql(MySqlRow),
}

impl Row {
    /// Gets the value at the given index and converts it to the given type.
    /// The index is zero-based.
    ///
    /// # Errors
    ///
    /// This method can return an error if the value at the given index is not
    /// compatible with the Rust type.
    ///
    /// This can also return an error if the index is out of bounds of the row
    /// returned by the database.
    pub fn get<T: FromDbValue>(&self, index: usize) -> Result<T> {
        let result = match self {
            #[cfg(feature = "sqlite")]
            Row::Sqlite(sqlite_row) => sqlite_row
                .get_raw(index)
                .and_then(|value| T::from_sqlite(value))?,
            #[cfg(feature = "postgres")]
            Row::Postgres(postgres_row) => postgres_row
                .get_raw(index)
                .and_then(|value| T::from_postgres(value))?,
            #[cfg(feature = "mysql")]
            Row::MySql(mysql_row) => mysql_row
                .get_raw(index)
                .and_then(|value| T::from_mysql(value))?,
        };

        Ok(result)
    }
}

/// A trait denoting that some type can be used as a field in a database.
pub trait DatabaseField: FromDbValue + ToDbFieldValue {
    const NULLABLE: bool = false;

    /// The type of the column in the database as one of the variants of
    /// the [`ColumnType`] enum.
    ///
    /// # Changing the column type after initial implementation
    ///
    /// Note that this should never be changed after the type is implemented.
    /// The migration generator is unable to detect a change in the column type
    /// and will not generate a migration for it. If the column type needs to
    /// be changed, a manual migration should be written, or a new type should
    /// be created.
    ///
    /// This is especially important for types that are stored as fixed-length
    /// strings in the database, as the migration generator cannot detect a
    /// change in the string length. For this reason, it's recommended to use
    /// the [`LimitedString`] type for fixed-length strings (which uses const
    /// generics, so each change in the length will be a new type) instead of
    /// a custom type with a fixed length.
    const TYPE: ColumnType;
}

/// A trait for converting a database value to a Rust value.
pub trait FromDbValue {
    /// Converts the given SQLite database value to a Rust value.
    ///
    /// # Errors
    ///
    /// This method can return an error if the value is not compatible with the
    /// Rust type.
    #[cfg(feature = "sqlite")]
    fn from_sqlite(value: SqliteValueRef) -> Result<Self>
    where
        Self: Sized;

    /// Converts the given PostgreSQL database value to a Rust value.
    ///
    /// # Errors
    ///
    /// This method can return an error if the value is not compatible with the
    /// Rust type.
    #[cfg(feature = "postgres")]
    fn from_postgres(value: PostgresValueRef) -> Result<Self>
    where
        Self: Sized;

    /// Converts the given MySQL database value to a Rust value.
    ///
    /// # Errors
    ///
    /// This method can return an error if the value is not compatible with the
    /// Rust type.
    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef) -> Result<Self>
    where
        Self: Sized;
}

pub type DbValue = sea_query::Value;

/// A trait for converting a Rust value to a database value.
pub trait ToDbValue: Send + Sync {
    /// Converts the Rust value to a `sea_query` value.
    ///
    /// This method is used to convert the Rust value to a value that can be
    /// used in a query.
    fn to_db_value(&self) -> DbValue;
}

/// A generalization of [`ToDbValue`] that can also return a marker that means a
/// value should be automatically generated by the database.
pub trait ToDbFieldValue {
    fn to_db_field_value(&self) -> DbFieldValue;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbFieldValue {
    /// The value should be automatically generated by the database and not
    /// included in the query.
    Auto,
    /// A value that should be included in the query.
    Value(DbValue),
}

impl DbFieldValue {
    #[must_use]
    pub fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    #[must_use]
    pub fn is_value(&self) -> bool {
        matches!(self, Self::Value(_))
    }

    #[must_use]
    pub fn unwrap_value(self) -> DbValue {
        self.expect_value("called DbValue::unwrap_value() on a wrong DbValue variant")
    }

    #[must_use]
    pub fn expect_value(self, message: &str) -> DbValue {
        match self {
            Self::Value(value) => value,
            Self::Auto => panic!("{message}"),
        }
    }
}

impl<T: ToDbValue> ToDbFieldValue for T {
    fn to_db_field_value(&self) -> DbFieldValue {
        DbFieldValue::Value(self.to_db_value())
    }
}

impl<T: Into<DbValue>> From<T> for DbFieldValue {
    fn from(value: T) -> Self {
        Self::Value(value.into())
    }
}

impl<T: ToDbValue + ?Sized> ToDbValue for &T {
    fn to_db_value(&self) -> DbValue {
        (*self).to_db_value()
    }
}

trait SqlxRowRef {
    type ValueRef<'r>: SqlxValueRef<'r>
    where
        Self: 'r;

    fn get_raw(&self, index: usize) -> Result<Self::ValueRef<'_>>;
}

pub trait SqlxValueRef<'r>: Sized {
    type DB: sqlx::Database;

    fn get_raw(self) -> <Self::DB as sqlx::Database>::ValueRef<'r>;

    fn get<T: sqlx::decode::Decode<'r, Self::DB> + Type<Self::DB>>(self) -> Result<T> {
        use sqlx::ValueRef;

        let value = self.get_raw();

        if !value.is_null() {
            let ty = value.type_info();

            if !ty.is_null() && !T::compatible(&ty) {
                return Err(DatabaseError::TypeMismatch {
                    expected: T::type_info().to_string(),
                    found: ty.to_string(),
                });
            }
        }
        T::decode(value).map_err(DatabaseError::ValueDecode)
    }
}

/// A database connection structure that holds the connection to the database.
///
/// It is used to execute queries and interact with the database. The connection
/// is established when the structure is created and closed when
/// [`Self::close()`] is called.
#[derive(Debug)]
pub struct Database {
    _url: String,
    inner: DatabaseImpl,
}

#[derive(Debug)]
enum DatabaseImpl {
    #[cfg(feature = "sqlite")]
    Sqlite(DatabaseSqlite),
    #[cfg(feature = "postgres")]
    Postgres(DatabasePostgres),
    #[cfg(feature = "mysql")]
    MySql(DatabaseMySql),
}

impl Database {
    /// Creates a new database connection. The connection string should be in
    /// the format of the database URL.
    ///
    /// # Errors
    ///
    /// This method can return an error if the connection to the database could
    /// not be established.
    ///
    /// This method can return an error if the database URL is not supported.
    ///
    /// This method can return an error if the database URL is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::db::Database;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let db = Database::new("sqlite::memory:").await.unwrap();
    /// }
    /// ```
    pub async fn new<T: Into<String>>(url: T) -> Result<Self> {
        let url = url.into();

        #[cfg(feature = "sqlite")]
        if url.starts_with("sqlite:") {
            let inner = DatabaseSqlite::new(&url).await?;
            return Ok(Self {
                _url: url,
                inner: DatabaseImpl::Sqlite(inner),
            });
        }

        #[cfg(feature = "postgres")]
        if url.starts_with("postgresql:") {
            let inner = DatabasePostgres::new(&url).await?;
            return Ok(Self {
                _url: url,
                inner: DatabaseImpl::Postgres(inner),
            });
        }

        #[cfg(feature = "mysql")]
        if url.starts_with("mysql:") {
            let inner = DatabaseMySql::new(&url).await?;
            return Ok(Self {
                _url: url,
                inner: DatabaseImpl::MySql(inner),
            });
        }

        panic!("Unsupported database URL: {url}");
    }

    /// Closes the database connection.
    ///
    /// This method should be called when the database connection is no longer
    /// needed. The connection is closed and the resources are released.
    ///
    /// # Errors
    ///
    /// This method can return an error if the connection to the database could
    /// not be closed gracefully, for instance because the connection has
    /// already been dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::db::Database;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let db = Database::new("sqlite::memory:").await.unwrap();
    ///     db.close().await.unwrap();
    /// }
    /// ```
    pub async fn close(&self) -> Result<()> {
        match &self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(inner) => inner.close().await,
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(inner) => inner.close().await,
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(inner) => inner.close().await,
        }
    }

    /// Inserts a new row into the database.
    ///
    /// # Errors
    ///
    /// This method can return an error if the row could not be inserted into
    /// the database, for instance because the migrations haven't been
    /// applied, or there was a problem with the database connection.
    pub async fn insert<T: Model>(&self, data: &mut T) -> Result<()> {
        let column_identifiers = T::COLUMNS
            .iter()
            .map(|column| Identifier::from(column.name.as_str()));
        let value_indices: Vec<_> = T::COLUMNS
            .iter()
            .enumerate()
            .map(|(i, _column)| i)
            .collect();
        let values = data
            .get_values(&value_indices)
            .into_iter()
            .map(ToDbFieldValue::to_db_field_value);

        let mut auto_col_ids = Vec::new();
        let mut auto_col_identifiers = Vec::new();
        let mut value_identifiers = Vec::new();
        let mut filtered_values = Vec::new();
        std::iter::zip(std::iter::zip(value_indices, column_identifiers), values).for_each(
            |((index, identifier), value)| match value {
                DbFieldValue::Auto => {
                    auto_col_ids.push(index);
                    auto_col_identifiers.push(identifier.into_column_ref());
                }
                DbFieldValue::Value(value) => {
                    value_identifiers.push(identifier);
                    filtered_values.push(value);
                }
            },
        );

        let mut insert_statement = sea_query::Query::insert()
            .into_table(T::TABLE_NAME)
            .columns(value_identifiers)
            .values(
                filtered_values
                    .into_iter()
                    .map(SimpleExpr::Value)
                    .collect::<Vec<_>>(),
            )?
            .or_default_values()
            .to_owned();

        if auto_col_ids.is_empty() {
            self.execute_statement(&insert_statement).await?;
        } else {
            let row = if self.supports_returning() {
                insert_statement.returning(ReturningClause::Columns(auto_col_identifiers));

                self.fetch_option(&insert_statement)
                    .await?
                    .expect("query should return the primary key")
            } else {
                let result = self.execute_statement(&insert_statement).await?;
                let row_id = result
                    .last_inserted_row_id
                    .expect("expected last inserted row ID if RETURNING clause is not supported");
                let query = sea_query::Query::select()
                    .from(T::TABLE_NAME)
                    .columns(auto_col_identifiers)
                    .and_where(sea_query::Expr::col(T::PRIMARY_KEY_NAME).eq(row_id))
                    .to_owned();
                self.fetch_option(&query).await?.expect(
                    "expected a row returned from a SELECT if RETURNING clause is not supported",
                )
            };
            data.update_from_db(row, &auto_col_ids)?;
        }

        debug!("Inserted row");

        Ok(())
    }

    /// Executes the given query and returns the results converted to the model
    /// type.
    ///
    /// # Errors
    ///
    /// This method can return an error if the query is invalid.
    ///
    /// This method can return an error if the data in the database is not
    /// compatible with the model (usually meaning the migrations haven't been
    /// generated or applied).
    ///
    /// Can return an error if the database connection is lost.
    pub async fn query<T: Model>(&self, query: &Query<T>) -> Result<Vec<T>> {
        let columns_to_get: Vec<_> = T::COLUMNS.iter().map(|column| column.name).collect();
        let mut select = sea_query::Query::select();
        select.columns(columns_to_get).from(T::TABLE_NAME);
        query.add_filter_to_statement(&mut select);

        let rows = self.fetch_all(&select).await?;
        let result = rows.into_iter().map(T::from_db).collect::<Result<_>>()?;

        Ok(result)
    }

    /// Returns the first row that matches the given query. If no rows match the
    /// query, returns `None`.
    ///
    /// # Errors
    ///
    /// This method can return an error if the query is invalid.
    ///
    /// This method can return an error if the model doesn't exist in the
    /// database (usually meaning the migrations haven't been generated or
    /// applied).
    ///
    /// Can return an error if the database connection is lost.
    pub async fn get<T: Model>(&self, query: &Query<T>) -> Result<Option<T>> {
        let columns_to_get: Vec<_> = T::COLUMNS.iter().map(|column| column.name).collect();
        let mut select = sea_query::Query::select();
        select.columns(columns_to_get).from(T::TABLE_NAME);
        query.add_filter_to_statement(&mut select);
        select.limit(1);

        let row = self.fetch_option(&select).await?;

        let result = match row {
            Some(row) => Some(T::from_db(row)?),
            None => None,
        };
        Ok(result)
    }

    /// Returns whether a row exists that matches the given query.
    ///
    /// # Errors
    ///
    /// This method can return an error if the query is invalid.
    ///
    /// This method can return an error if the model doesn't exist in the
    /// database (usually meaning the migrations haven't been generated or
    /// applied).
    ///
    /// Can return an error if the database connection is lost.
    pub async fn exists<T: Model>(&self, query: &Query<T>) -> Result<bool> {
        let mut select = sea_query::Query::select();
        select.expr(sea_query::Expr::value(1)).from(T::TABLE_NAME);
        query.add_filter_to_statement(&mut select);
        select.limit(1);

        let rows = self.fetch_option(&select).await?;

        Ok(rows.is_some())
    }

    /// Deletes all rows that match the given query.
    ///
    /// # Errors
    ///
    /// This method can return an error if the query is invalid.
    ///
    /// This method can return an error if the model doesn't exist in the
    /// database (usually meaning the migrations haven't been generated or
    /// applied).
    ///
    /// Can return an error if the database connection is lost.
    pub async fn delete<T: Model>(&self, query: &Query<T>) -> Result<StatementResult> {
        let mut delete = sea_query::Query::delete();
        delete.from_table(T::TABLE_NAME);
        query.add_filter_to_statement(&mut delete);

        self.execute_statement(&delete).await
    }

    pub async fn raw(&self, query: &str) -> Result<StatementResult> {
        self.raw_with(query, &[]).await
    }

    pub async fn raw_with(
        &self,
        query: &str,
        values: &[&(dyn ToDbValue)],
    ) -> Result<StatementResult> {
        let values = values
            .iter()
            .map(ToDbValue::to_db_value)
            .collect::<Vec<_>>();
        let values = SqlxValues(sea_query::Values(values));

        let result = match &self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(inner) => inner.raw_with(query, values).await?,
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(inner) => inner.raw_with(query, values).await?,
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(inner) => inner.raw_with(query, values).await?,
        };

        Ok(result)
    }

    async fn fetch_option<T>(&self, statement: &T) -> Result<Option<Row>>
    where
        T: SqlxBinder,
    {
        let result = match &self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(inner) => inner.fetch_option(statement).await?.map(Row::Sqlite),
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(inner) => {
                inner.fetch_option(statement).await?.map(Row::Postgres)
            }
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(inner) => inner.fetch_option(statement).await?.map(Row::MySql),
        };

        Ok(result)
    }

    fn supports_returning(&self) -> bool {
        match self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(_) => true,
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(_) => true,
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(_) => false,
        }
    }

    async fn fetch_all<T>(&self, statement: &T) -> Result<Vec<Row>>
    where
        T: SqlxBinder,
    {
        let result = match &self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(inner) => inner
                .fetch_all(statement)
                .await?
                .into_iter()
                .map(Row::Sqlite)
                .collect(),
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(inner) => inner
                .fetch_all(statement)
                .await?
                .into_iter()
                .map(Row::Postgres)
                .collect(),
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(inner) => inner
                .fetch_all(statement)
                .await?
                .into_iter()
                .map(Row::MySql)
                .collect(),
        };

        Ok(result)
    }

    async fn execute_statement<T>(&self, statement: &T) -> Result<StatementResult>
    where
        T: SqlxBinder + Sync,
    {
        let result = match &self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(inner) => inner.execute_statement(statement).await?,
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(inner) => inner.execute_statement(statement).await?,
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(inner) => inner.execute_statement(statement).await?,
        };

        Ok(result)
    }

    async fn execute_schema<T: SchemaStatementBuilder>(
        &self,
        statement: T,
    ) -> Result<StatementResult> {
        let result = match &self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(inner) => inner.execute_schema(statement).await?,
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(inner) => inner.execute_schema(statement).await?,
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(inner) => inner.execute_schema(statement).await?,
        };

        Ok(result)
    }
}

impl ColumnTypeMapper for Database {
    fn sea_query_column_type_for(&self, column_type: ColumnType) -> sea_query::ColumnType {
        match &self.inner {
            #[cfg(feature = "sqlite")]
            DatabaseImpl::Sqlite(inner) => inner.sea_query_column_type_for(column_type),
            #[cfg(feature = "postgres")]
            DatabaseImpl::Postgres(inner) => inner.sea_query_column_type_for(column_type),
            #[cfg(feature = "mysql")]
            DatabaseImpl::MySql(inner) => inner.sea_query_column_type_for(column_type),
        }
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    async fn insert<T: Model>(&self, data: &mut T) -> Result<()>;

    async fn query<T: Model>(&self, query: &Query<T>) -> Result<Vec<T>>;

    async fn get<T: Model>(&self, query: &Query<T>) -> Result<Option<T>>;

    async fn exists<T: Model>(&self, query: &Query<T>) -> Result<bool>;

    async fn delete<T: Model>(&self, query: &Query<T>) -> Result<StatementResult>;
}

#[async_trait]
impl DatabaseBackend for Database {
    async fn insert<T: Model>(&self, data: &mut T) -> Result<()> {
        Database::insert(self, data).await
    }

    async fn query<T: Model>(&self, query: &Query<T>) -> Result<Vec<T>> {
        Database::query(self, query).await
    }

    async fn get<T: Model>(&self, query: &Query<T>) -> Result<Option<T>> {
        Database::get(self, query).await
    }

    async fn exists<T: Model>(&self, query: &Query<T>) -> Result<bool> {
        Database::exists(self, query).await
    }

    async fn delete<T: Model>(&self, query: &Query<T>) -> Result<StatementResult> {
        Database::delete(self, query).await
    }
}

/// Result of a statement execution.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatementResult {
    rows_affected: RowsNum,
    last_inserted_row_id: Option<u64>,
}

impl StatementResult {
    /// Creates a new statement result with the given number of rows affected.
    #[must_use]
    #[cfg(test)]
    fn new(rows_affected: RowsNum) -> Self {
        Self {
            rows_affected,
            last_inserted_row_id: None,
        }
    }

    /// Returns the number of rows affected by the query.
    #[must_use]
    pub fn rows_affected(&self) -> RowsNum {
        self.rows_affected
    }

    /// Returns the ID of the last inserted row.
    #[must_use]
    pub fn last_inserted_row_id(&self) -> Option<u64> {
        self.last_inserted_row_id
    }
}

/// A structure that holds the number of rows.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, Display)]
pub struct RowsNum(pub u64);

/// A wrapper over a value that can be either a fixed value or be automatically
/// generated by the database.
///
/// This is primarily used for auto-incrementing primary keys.
///
/// # Examples
///
/// ```
/// use flareon::db::{model, Auto, Model};
/// # use flareon::db::migrations::{Field, Operation};
/// # use flareon::db::{Database, Identifier, DatabaseField};
/// # use flareon::Result;
///
/// #[model]
/// struct MyModel {
///     id: Auto<i32>,
/// }
///
/// # #[tokio::main]
/// # async fn main() -> Result<()> {
///
/// # const OPERATION: Operation = Operation::create_model()
/// #     .table_name(Identifier::new("my_model"))
/// #     .fields(&[
/// #         Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
/// #             .primary_key()
/// #             .auto(),
/// #     ])
/// #     .build();
///
/// let database = Database::new("sqlite::memory:").await?;
/// # OPERATION.forwards(&database).await?;
///
/// let mut my_model = MyModel { id: Auto::auto() };
/// my_model.save(&database).await?;
/// assert!(matches!(my_model.id, Auto::Fixed(_)));
///
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Auto<T> {
    /// A fixed value.
    Fixed(T),
    /// A value that will be automatically generated by the database.
    Auto,
}

impl<T> Auto<T> {
    /// Creates a new `Auto` instance that is automatically generated by the
    /// database.
    #[must_use]
    #[allow(clippy::self_named_constructors)]
    pub const fn auto() -> Self {
        Self::Auto
    }

    /// Creates a new `Auto` instance with a fixed value.
    #[must_use]
    pub const fn fixed(value: T) -> Self {
        Self::Fixed(value)
    }
}

impl<T> Default for Auto<T> {
    fn default() -> Self {
        Self::Auto
    }
}

impl<T> From<T> for Auto<T> {
    fn from(value: T) -> Self {
        Self::fixed(value)
    }
}

/// A wrapper over a string that has a limited length.
///
/// This type is used to represent a string that has a limited length in the
/// database. The length is specified as a const generic parameter. The string
/// is stored as a normal string in memory, but it is checked when it is
/// created to ensure that it is not longer than the specified limit.
///
/// # Database
///
/// This type is represented by the `VARCHAR` type in the database, with the
/// maximum length same as the limit specified in the type.
///
/// # Examples
///
/// ```
/// use flareon::db::LimitedString;
///
/// let limited_string = LimitedString::<5>::new("test").unwrap();
/// assert_eq!(limited_string, "test");
///
/// let limited_string = LimitedString::<5>::new("too long");
/// assert!(limited_string.is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref)]
pub struct LimitedString<const LIMIT: u32>(String);

impl<const LIMIT: u32> PartialEq<&str> for LimitedString<LIMIT> {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}
impl<const LIMIT: u32> PartialEq<String> for LimitedString<LIMIT> {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}
impl<const LIMIT: u32> PartialEq<LimitedString<LIMIT>> for &str {
    fn eq(&self, other: &LimitedString<LIMIT>) -> bool {
        *self == other.0
    }
}
impl<const LIMIT: u32> PartialEq<LimitedString<LIMIT>> for String {
    fn eq(&self, other: &LimitedString<LIMIT>) -> bool {
        *self == other.0
    }
}

/// An error returned by [`LimitedString::new`] when the string is longer than
/// the specified limit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Error)]
#[error("string is too long ({length} > {LIMIT})")]
pub struct NewLimitedStringError<const LIMIT: u32> {
    pub(crate) length: u32,
}

impl<const LIMIT: u32> LimitedString<LIMIT> {
    pub fn new(
        value: impl Into<String>,
    ) -> std::result::Result<Self, NewLimitedStringError<LIMIT>> {
        let value = value.into();
        let length = value.len() as u32;

        if length > LIMIT {
            return Err(NewLimitedStringError { length });
        }
        Ok(Self(value))
    }
}

#[cfg(feature = "fake")]
impl<const LIMIT: u32> fake::Dummy<usize> for LimitedString<LIMIT> {
    fn dummy_with_rng<R: fake::rand::Rng + ?Sized>(len: &usize, rng: &mut R) -> Self {
        use fake::rand::Rng;

        assert!(
            *len <= LIMIT as usize,
            concat!(
                "len must be less than or equal to LIMIT (",
                stringify!(LIMIT),
                ")"
            )
        );

        let str: String = rng
            .sample_iter(&fake::rand::distributions::Alphanumeric)
            .take(*len)
            .map(char::from)
            .collect();
        Self::new(str).unwrap()
    }
}

#[cfg(feature = "fake")]
impl<const LIMIT: u32> fake::Dummy<fake::Faker> for LimitedString<LIMIT> {
    fn dummy_with_rng<R: fake::rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
        use fake::Fake;

        let len: usize = (0..LIMIT as usize).fake_with_rng(rng);
        len.fake_with_rng(rng)
    }
}

/// A type that represents a column type in the database.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnType {
    Boolean,
    TinyInteger,
    SmallInteger,
    Integer,
    BigInteger,
    TinyUnsignedInteger,
    SmallUnsignedInteger,
    UnsignedInteger,
    BigUnsignedInteger,
    Float,
    Double,
    Time,
    Date,
    DateTime,
    DateTimeWithTimeZone,
    Text,
    Blob,
    String(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier() {
        let id = Identifier::new("test");
        assert_eq!(id.as_str(), "test");
    }

    #[test]
    fn column() {
        let column = Column::new(Identifier::new("test"));
        assert_eq!(column.name.as_str(), "test");
    }

    #[test]
    fn limited_string_new_within_limit() {
        let limited_string = LimitedString::<10>::new("short");
        assert!(limited_string.is_ok());
        assert_eq!(limited_string.unwrap(), "short");
    }

    #[test]
    fn limited_string_new_exceeds_limit() {
        let limited_string = LimitedString::<5>::new("too long");

        assert!(limited_string.is_err());
        let error = limited_string.unwrap_err();
        assert_eq!(error.to_string(), "string is too long (8 > 5)");
    }

    #[test]
    fn limited_string_new_exact_limit() {
        let limited_string = LimitedString::<5>::new("exact");
        assert!(limited_string.is_ok());
        assert_eq!(limited_string.unwrap(), "exact");
    }

    #[test]
    fn limited_string_eq() {
        assert_eq!(LimitedString::<5>::new("test").unwrap(), "test");
        assert_eq!("test", LimitedString::<5>::new("test").unwrap());
        assert_eq!(
            LimitedString::<5>::new("test").unwrap(),
            String::from("test"),
        );
        assert_eq!(
            String::from("test"),
            LimitedString::<5>::new("test").unwrap(),
        );
    }

    #[test]
    fn db_field_value_is_auto() {
        let auto_value = DbFieldValue::Auto;
        assert!(auto_value.is_auto());
        assert!(!auto_value.is_value());
    }

    #[test]
    fn db_field_value_is_value() {
        let value = DbFieldValue::Value(42.into());
        assert!(value.is_value());
        assert!(!value.is_auto());
    }

    #[test]
    fn db_field_value_unwrap() {
        let value = DbFieldValue::Value(42.into());
        assert_eq!(value.unwrap_value(), 42.into());
    }

    #[test]
    #[should_panic(expected = "called DbValue::unwrap_value() on a wrong DbValue variant")]
    fn db_field_value_unwrap_panic() {
        let auto_value = DbFieldValue::Auto;
        let _ = auto_value.unwrap_value();
    }

    #[test]
    fn db_field_value_expect() {
        let value = DbFieldValue::Value(42.into());
        assert_eq!(value.expect_value("expected a value"), 42.into());
    }

    #[test]
    #[should_panic(expected = "expected a value")]
    fn db_field_value_expect_panic() {
        let auto_value = DbFieldValue::Auto;
        let _ = auto_value.expect_value("expected a value");
    }
}
