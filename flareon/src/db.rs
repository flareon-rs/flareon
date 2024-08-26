mod fields;
pub mod migrations;
pub mod query;

use std::fmt::Write;
use std::hash::Hash;

use async_trait::async_trait;
use derive_more::{Debug, Deref, Display};
pub use flareon_macros::model;
use log::debug;
use query::Query;
use sea_query::{
    Iden, QueryBuilder, SchemaBuilder, SchemaStatementBuilder, SimpleExpr, SqliteQueryBuilder,
};
use sea_query_binder::SqlxBinder;
use sqlx::any::AnyPoolOptions;
use sqlx::pool::PoolConnection;
use sqlx::{AnyPool, Type, TypeInfo};
use thiserror::Error;

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
#[derive(Debug, Clone)]
pub enum Identifier {
    Static(&'static str),
    Owned(String),
}

impl Identifier {
    /// Creates a new identifier from a static string.
    #[must_use]
    pub const fn new(s: &'static str) -> Self {
        Self::Static(s)
    }

    /// Creates a new identifier from a string.
    #[must_use]
    pub const fn from_string(s: String) -> Self {
        Self::Owned(s)
    }

    /// Returns the inner string of the identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Static(s) => s,
            Self::Owned(s) => s,
        }
    }
}

impl From<&'static str> for Identifier {
    fn from(s: &'static str) -> Self {
        Self::new(s)
    }
}

impl PartialEq for Identifier {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for Identifier {}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.as_str().cmp(other.as_str()))
    }
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for Identifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    pub const fn auto(mut self, auto_value: bool) -> Self {
        self.auto_value = auto_value;
        self
    }

    #[must_use]
    pub const fn null(mut self, null: bool) -> Self {
        self.null = null;
        self
    }
}

/// A row structure that holds the data of a single row retrieved from the
/// database.
#[derive(Debug)]
pub struct Row {
    #[debug("...")]
    inner: sqlx::any::AnyRow,
}

impl Row {
    #[must_use]
    fn new(inner: sqlx::any::AnyRow) -> Self {
        Self { inner }
    }

    pub fn get<T: FromDbValue>(&self, index: usize) -> Result<T> {
        let value = SqlxValueRef::new(sqlx::Row::try_get_raw(&self.inner, index)?);
        Ok(T::from_sqlx(value)?)
    }
}

pub trait DbField: FromDbValue + ToDbValue {
    const TYPE: ColumnType;
}

/// A trait for converting a database value to a Rust value.
pub trait FromDbValue: Sized {
    fn from_sqlx(value: SqlxValueRef) -> Result<Self>;
}

/// A trait for converting a Rust value to a database value.
pub trait ToDbValue: Send + Sync {
    fn as_sea_query_value(&self) -> sea_query::Value;
}

#[derive(Debug)]
pub struct SqlxValueRef<'r> {
    inner: sqlx::any::AnyValueRef<'r>,
}

impl<'r> SqlxValueRef<'r> {
    #[must_use]
    fn new(inner: sqlx::any::AnyValueRef<'r>) -> Self {
        Self { inner }
    }

    pub fn get<T: sqlx::decode::Decode<'r, sqlx::any::Any> + Type<sqlx::any::Any>>(
        self,
    ) -> Result<T> {
        use sqlx::ValueRef;

        let value = self.inner;

        if !value.is_null() {
            let ty = value.type_info();

            if !ty.is_null() && !T::compatible(&ty) {
                return Err(DatabaseError::TypeMismatch {
                    expected: T::type_info().to_string(),
                    found: ty.to_string(),
                });
            }
        }
        T::decode(value).map_err(|source| DatabaseError::ValueDecode(source))
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
    db_connection: AnyPool,
    #[debug("...")]
    query_builder: Box<dyn QueryBuilder + Send + Sync>,
    #[debug("...")]
    schema_builder: Box<dyn SchemaBuilder + Send + Sync>,
}

impl Database {
    pub async fn new<T: Into<String>>(url: T) -> Result<Self> {
        sqlx::any::install_default_drivers();

        let url = url.into();

        let db_conn = AnyPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await?;
        let (query_builder, schema_builder): (
            Box<dyn QueryBuilder + Send + Sync>,
            Box<dyn SchemaBuilder + Send + Sync>,
        ) = {
            if url.starts_with("sqlite:") {
                (Box::new(SqliteQueryBuilder), Box::new(SqliteQueryBuilder))
            } else {
                todo!("Other databases are not supported yet");
            }
        };

        Ok(Self {
            _url: url,
            db_connection: db_conn,
            query_builder,
            schema_builder,
        })
    }

    pub async fn close(self) -> Result<()> {
        self.db_connection.close().await;
        Ok(())
    }

    pub async fn execute(&self, query: &str) -> Result<QueryResult> {
        let sqlx_query = sqlx::query(query);

        self.execute_sqlx(sqlx_query).await
    }

    async fn execute_sqlx<'a, A>(
        &self,
        sqlx_query: sqlx::query::Query<'a, sqlx::any::Any, A>,
    ) -> Result<QueryResult>
    where
        A: 'a + sqlx::IntoArguments<'a, sqlx::any::Any>,
    {
        let result = sqlx_query.execute(&mut *self.conn().await?).await?;
        let result = QueryResult {
            rows_affected: RowsNum(result.rows_affected()),
        };

        debug!("Rows affected: {}", result.rows_affected.0);
        Ok(result)
    }

    async fn conn(&self) -> Result<PoolConnection<sqlx::any::Any>> {
        Ok(self.db_connection.acquire().await?)
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

        let (sql, values) = sea_query::Query::insert()
            .into_table(T::TABLE_NAME)
            .columns(non_auto_column_identifiers)
            .values(
                values
                    .into_iter()
                    .map(|value| SimpleExpr::Value(value.as_sea_query_value()))
                    .collect::<Vec<_>>(),
            )?
            .returning_col(Identifier::new("id"))
            .build_any_sqlx(self.query_builder.as_ref());

        debug!("Insert query: {}", sql);

        let row = sqlx::query_with(&sql, values)
            .fetch_one(&mut *self.conn().await?)
            .await?;
        let id: i64 = sqlx::Row::try_get(&row, 0)?;
        debug!("Inserted row with id: {}", id);

        Ok(())
    }

    pub async fn query<T: Model>(&self, query: &Query<T>) -> Result<Vec<T>> {
        let columns_to_get: Vec<_> = T::COLUMNS
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let mut select = sea_query::Query::select();
        select.columns(columns_to_get).from(T::TABLE_NAME);
        query.modify_statement(&mut select);
        let (sql, values) = select.build_any_sqlx(self.query_builder.as_ref());

        debug!("Select query: {}", sql);

        let rows: Vec<T> = sqlx::query_with(&sql, values)
            .fetch_all(&mut *self.conn().await?)
            .await?
            .into_iter()
            .map(|row| T::from_db(Row::new(row)))
            .collect::<Result<_>>()?;

        Ok(rows)
    }

    pub async fn delete<T: Model>(&self, query: &Query<T>) -> Result<QueryResult> {
        let mut delete = sea_query::Query::delete();
        delete.from_table(T::TABLE_NAME);
        query.modify_statement(&mut delete);
        let (sql, values) = delete.build_any_sqlx(self.query_builder.as_ref());

        debug!("Delete query: {}", sql);

        self.execute_sqlx(sqlx::query_with(&sql, values)).await
    }

    pub async fn execute_schema<T: SchemaStatementBuilder>(
        &self,
        statement: T,
    ) -> Result<QueryResult> {
        let sql = statement.build_any(self.schema_builder.as_ref());

        debug!("Schema modification: {}", sql);

        self.execute_sqlx(sqlx::query(&sql)).await
    }
}

/// Result of a query execution.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryResult {
    rows_affected: RowsNum,
}

impl QueryResult {
    /// Returns the number of rows affected by the query.
    #[must_use]
    pub fn rows_affected(&self) -> RowsNum {
        self.rows_affected
    }
}

/// A structure that holds the number of rows.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, Display)]
pub struct RowsNum(pub u64);

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
    }

    #[derive(std::fmt::Debug, PartialEq)]
    #[model]
    struct TestModel {
        id: i32,
        name: String,
    }

    #[tokio::test]
    async fn test_model_crud() {
        let db = test_db().await;

        db.execute(
            r"
        CREATE TABLE test_model (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL
        );",
        )
        .await
        .unwrap();

        assert_eq!(TestModel::objects().all(&db).await.unwrap(), vec![]);

        let mut model = TestModel {
            id: 0,
            name: "test".to_owned(),
        };
        model.save(&db).await.unwrap();

        assert_eq!(TestModel::objects().all(&db).await.unwrap().len(), 1);

        use crate::db::query::ExprEq;
        TestModel::objects()
            .filter(<TestModel as Model>::Fields::ID.eq(1))
            .delete(&db)
            .await
            .unwrap();

        assert_eq!(TestModel::objects().all(&db).await.unwrap(), vec![]);
    }

    async fn test_db() -> Database {
        Database::new("sqlite::memory:").await.unwrap()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnType {
    TinyInteger,
    SmallInteger,
    Integer,
    BigInteger,
    TinyUnsignedInteger,
    SmallUnsignedInteger,
    UnsignedInteger,
    BigUnsignedInteger,
    Text,
}
