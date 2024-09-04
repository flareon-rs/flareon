pub mod db_sqlite;
mod fields;
pub mod migrations;
pub mod query;

use std::fmt::Write;
use std::hash::Hash;

use async_trait::async_trait;
use derive_more::{Debug, Deref, Display};
pub use flareon_macros::{model, query};
use log::debug;
use query::Query;
use sea_query::{Iden, SchemaStatementBuilder, SimpleExpr};
use sea_query_binder::SqlxBinder;
use sqlx::{Type, TypeInfo};
use thiserror::Error;

use crate::db::db_sqlite::{DatabaseSqlite, SqliteRow, SqliteValueRef};

/// An error that can occur when interacting with the database.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DatabaseError {
    #[error("Database engine error: {0}")]
    DatabaseEngineError(#[from] sqlx::Error),
    #[error("Error when building query: {0}")]
    QueryBuildingError(#[from] sea_query::error::Error),
    #[error("Type mismatch in database value: expected `{expected}`, found `{found}`. Perhaps migration is needed."
    )]
    TypeMismatch { expected: String, found: String },
    #[error("Error when decoding database value: {0}")]
    ValueDecode(Box<dyn std::error::Error + 'static + Send + Sync>),
}

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
///    id: i32,
///    name: String,
/// }
/// ```
#[async_trait]
pub trait Model: Sized + Send {
    /// A helper structure for the fields of the model.
    ///
    /// This structure should a constant [`FieldRef`](query::FieldRef) instance
    /// for each field in the model. Note that the names of the fields
    /// should be written in UPPER_SNAKE_CASE, just like other constants in
    /// Rust.
    type Fields;

    /// The name of the table in the database.
    const TABLE_NAME: Identifier;

    /// The columns of the model.
    const COLUMNS: &'static [Column];

    /// Creates a model instance from a database row.
    fn from_db(db_row: Row) -> Result<Self>;

    /// Gets the values of the model for the given columns.
    fn get_values(&self, columns: &[usize]) -> Vec<&dyn ToDbValue>;

    /// Returns a query for all objects of this model.
    #[must_use]
    fn objects() -> Query<Self> {
        Query::new()
    }

    /// Saves the model to the database.
    async fn save(&mut self, db: &Database) -> Result<()> {
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
    auto_value: bool,
    null: bool,
}

impl Column {
    #[must_use]
    pub const fn new(name: Identifier) -> Self {
        Self {
            name,
            auto_value: false,
            null: false,
        }
    }

    #[must_use]
    pub const fn auto(mut self) -> Self {
        self.auto_value = true;
        self
    }

    #[must_use]
    pub const fn null(mut self) -> Self {
        self.null = true;
        self
    }
}

/// A row structure that holds the data of a single row retrieved from the
/// database.
#[derive(Debug)]
pub enum Row {
    Sqlite(SqliteRow),
}

impl Row {
    pub fn get<T: FromDbValue>(&self, index: usize) -> Result<T> {
        let result = match self {
            Row::Sqlite(sqlite_row) => sqlite_row
                .get_raw(index)
                .and_then(|value| T::from_sqlite(value))?,
        };

        Ok(result)
    }
}

pub trait DbField: FromDbValue + ToDbValue {
    const TYPE: ColumnType;
}

/// A trait for converting a database value to a Rust value.
pub trait FromDbValue: Sized {
    fn from_sqlite(value: SqliteValueRef) -> Result<Self>;
}

/// A trait for converting a Rust value to a database value.
pub trait ToDbValue: Send + Sync {
    fn as_sea_query_value(&self) -> sea_query::Value;
}

trait SqlxRowRef {
    type ValueRef<'r>: SqlxValueRef<'r>
    where
        Self: 'r;

    fn get_raw(&self, index: usize) -> Result<Self::ValueRef<'_>>;
}

trait SqlxValueRef<'r>: Sized {
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
    Sqlite(DatabaseSqlite),
}

impl Database {
    pub async fn new<T: Into<String>>(url: T) -> Result<Self> {
        let url = url.into();
        let db = if url.starts_with("sqlite:") {
            let inner = DatabaseSqlite::new(&url).await?;
            Self {
                _url: url,
                inner: DatabaseImpl::Sqlite(inner),
            }
        } else {
            todo!("Other databases are not supported yet");
        };

        Ok(db)
    }

    pub async fn close(self) -> Result<()> {
        match self.inner {
            DatabaseImpl::Sqlite(inner) => inner.close().await,
        }
    }

    pub async fn insert<T: Model>(&self, data: &mut T) -> Result<()> {
        let non_auto_column_identifiers = T::COLUMNS
            .iter()
            .filter_map(|column| {
                if column.auto_value {
                    None
                } else {
                    Some(Identifier::from(column.name.as_str()))
                }
            })
            .collect::<Vec<_>>();
        let value_indices = T::COLUMNS
            .iter()
            .enumerate()
            .filter_map(|(i, column)| if column.auto_value { None } else { Some(i) })
            .collect::<Vec<_>>();
        let values = data.get_values(&value_indices);

        let insert_statement = sea_query::Query::insert()
            .into_table(T::TABLE_NAME)
            .columns(non_auto_column_identifiers)
            .values(
                values
                    .into_iter()
                    .map(|value| SimpleExpr::Value(value.as_sea_query_value()))
                    .collect::<Vec<_>>(),
            )?
            .returning_col(Identifier::new("id"))
            .to_owned();

        let row = self.fetch_one(insert_statement).await?;
        let id = row.get::<i64>(0)?;

        debug!("Inserted row with id: {}", id);

        Ok(())
    }

    pub async fn query<T: Model>(&self, query: &Query<T>) -> Result<Vec<T>> {
        let columns_to_get: Vec<_> = T::COLUMNS.iter().map(|column| column.name).collect();
        let mut select = sea_query::Query::select();
        select.columns(columns_to_get).from(T::TABLE_NAME);
        query.modify_statement(&mut select);

        let rows = self.fetch_all(select).await?;
        let result = rows.into_iter().map(T::from_db).collect::<Result<_>>()?;

        Ok(result)
    }

    pub async fn exists<T: Model>(&self, query: &Query<T>) -> Result<bool> {
        let mut select = sea_query::Query::select();
        select.expr(sea_query::Expr::value(1)).from(T::TABLE_NAME);
        query.modify_statement(&mut select);
        select.limit(1);

        let rows = self.fetch_option(select).await?;

        Ok(rows.is_some())
    }

    pub async fn delete<T: Model>(&self, query: &Query<T>) -> Result<StatementResult> {
        let mut delete = sea_query::Query::delete();
        delete.from_table(T::TABLE_NAME);
        query.modify_statement(&mut delete);

        self.execute_statement(delete).await
    }

    async fn fetch_one<T>(&self, statement: T) -> Result<Row>
    where
        T: SqlxBinder,
    {
        let result = match &self.inner {
            DatabaseImpl::Sqlite(inner) => Row::Sqlite(inner.fetch_one(statement).await?),
        };

        Ok(result)
    }

    async fn fetch_option<T>(&self, statement: T) -> Result<Option<Row>>
    where
        T: SqlxBinder,
    {
        let result = match &self.inner {
            DatabaseImpl::Sqlite(inner) => inner.fetch_option(statement).await?.map(Row::Sqlite),
        };

        Ok(result)
    }

    async fn fetch_all<T>(&self, statement: T) -> Result<Vec<Row>>
    where
        T: SqlxBinder,
    {
        let result = match &self.inner {
            DatabaseImpl::Sqlite(inner) => inner
                .fetch_all(statement)
                .await?
                .into_iter()
                .map(Row::Sqlite)
                .collect(),
        };

        Ok(result)
    }

    async fn execute_statement<T>(&self, statement: T) -> Result<StatementResult>
    where
        T: SqlxBinder,
    {
        let result = match &self.inner {
            DatabaseImpl::Sqlite(inner) => inner.execute_statement(statement).await?,
        };

        Ok(result)
    }

    async fn execute_schema<T: SchemaStatementBuilder>(
        &self,
        statement: T,
    ) -> Result<StatementResult> {
        let result = match &self.inner {
            DatabaseImpl::Sqlite(inner) => inner.execute_schema(statement).await?,
        };

        Ok(result)
    }
}

/// Result of a statement execution.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatementResult {
    rows_affected: RowsNum,
}

impl StatementResult {
    /// Returns the number of rows affected by the query.
    #[must_use]
    pub fn rows_affected(&self) -> RowsNum {
        self.rows_affected
    }
}

/// A structure that holds the number of rows.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, Display)]
pub struct RowsNum(pub u64);

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
    Timestamp,
    TimestampWithTimeZone,
    Text,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identifier() {
        let id = Identifier::new("test");
        assert_eq!(id.as_str(), "test");
    }

    #[test]
    fn test_column() {
        let column = Column::new(Identifier::new("test"));
        assert_eq!(column.name.as_str(), "test");
        assert!(!column.auto_value);
        assert!(!column.null);

        let column_auto = column.auto();
        assert!(column_auto.auto_value);

        let column_null = column.null();
        assert!(column_null.null);
    }
}
