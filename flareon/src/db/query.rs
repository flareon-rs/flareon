use std::marker::PhantomData;

use derive_more::Debug;
use sea_query::IntoColumnRef;

use crate::db;
use crate::db::{DatabaseBackend, FromDbValue, Identifier, Model, StatementResult, ToDbValue};

/// A query that can be executed on a database. Can be used to filter, update,
/// or delete rows.
///
/// # Example
/// ```
/// use flareon::db::model;
/// use flareon::db::query::Query;
///
/// #[model]
/// struct User {
///     id: i32,
///     name: String,
///     age: i32,
/// }
///
/// let query = Query::<User>::new();
/// ```
#[derive(Debug)]
pub struct Query<T> {
    filter: Option<Expr>,
    phantom_data: PhantomData<fn() -> T>,
}

impl<T: Model> Default for Query<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Model> Query<T> {
    /// Create a new query.
    ///
    /// # Example
    /// ```
    /// use flareon::db::model;
    /// use flareon::db::query::Query;
    ///
    /// #[model]
    /// struct User {
    ///     id: i32,
    ///     name: String,
    ///     age: i32,
    /// }
    ///
    /// let query = Query::<User>::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            filter: None,
            phantom_data: PhantomData,
        }
    }

    /// Set the filter expression for the query.
    ///
    /// # Example
    /// ```
    /// use flareon::db::model;
    /// use flareon::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct User {
    ///     id: i32,
    ///     name: String,
    ///     age: i32,
    /// }
    ///
    /// let query = Query::<User>::new().filter(Expr::eq(Expr::field("name"), Expr::value("John")));
    /// ```
    pub fn filter(&mut self, filter: Expr) -> &mut Self {
        self.filter = Some(filter);
        self
    }

    /// Execute the query and return all results.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn all<DB: DatabaseBackend>(&self, db: &DB) -> db::Result<Vec<T>> {
        db.query(self).await
    }

    /// Execute the query and return the first result.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn get<DB: DatabaseBackend>(&self, db: &DB) -> db::Result<Option<T>> {
        // TODO panic/error if more than one result
        db.get(self).await
    }

    /// Execute the query and check if any results exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn exists<DB: DatabaseBackend>(&self, db: &DB) -> db::Result<bool> {
        db.exists(self).await
    }

    /// Delete all rows that match the query.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn delete<DB: DatabaseBackend>(&self, db: &DB) -> db::Result<StatementResult> {
        db.delete(self).await
    }

    pub(super) fn add_filter_to_statement<S: sea_query::ConditionalStatement>(
        &self,
        statement: &mut S,
    ) {
        if let Some(filter) = &self.filter {
            statement.and_where(filter.as_sea_query_expr());
        }
    }
}

#[derive(Debug)]
pub enum Expr {
    Field(Identifier),
    Value(#[debug("{}", _0.to_sea_query_value())] Box<dyn ToDbValue>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Create a new field expression. This represents a reference to a column
    /// in the database.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::field("name");
    /// ```
    #[must_use]
    pub fn field<T: Into<Identifier>>(identifier: T) -> Self {
        Self::Field(identifier.into())
    }

    /// Create a new value expression. This represents a literal value that gets
    /// passed into the SQL query.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::value(30);
    /// ```
    #[must_use]
    pub fn value<T: ToDbValue + 'static>(value: T) -> Self {
        Self::Value(Box::new(value))
    }

    /// Create a new `AND` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::and(
    ///     Expr::eq(Expr::field("name"), Expr::value("John")),
    ///     Expr::eq(Expr::field("age"), Expr::value(30)),
    /// );
    /// ```
    #[must_use]
    pub fn and(lhs: Self, rhs: Self) -> Self {
        Self::And(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `OR` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::or(
    ///     Expr::eq(Expr::field("name"), Expr::value("John")),
    ///     Expr::eq(Expr::field("age"), Expr::value(30)),
    /// );
    /// ```
    #[must_use]
    pub fn or(lhs: Self, rhs: Self) -> Self {
        Self::Or(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::eq(Expr::field("name"), Expr::value("John"));
    /// ```
    #[must_use]
    pub fn eq(lhs: Self, rhs: Self) -> Self {
        Self::Eq(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `!=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::ne(Expr::field("name"), Expr::value("John"));
    /// ```
    #[must_use]
    pub fn ne(lhs: Self, rhs: Self) -> Self {
        Self::Ne(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `+` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::add(Expr::field("age"), Expr::value(10));
    /// ```
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn add(lhs: Self, rhs: Self) -> Self {
        Self::Add(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `-` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::sub(Expr::field("age"), Expr::value(10));
    /// ```
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn sub(lhs: Self, rhs: Self) -> Self {
        Self::Sub(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `*` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::mul(Expr::field("amount"), Expr::value(5));
    /// ```
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn mul(lhs: Self, rhs: Self) -> Self {
        Self::Mul(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `/` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::db::query::Expr;
    ///
    /// let expr = Expr::div(Expr::field("amount"), Expr::value(5));
    /// ```
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn div(lhs: Self, rhs: Self) -> Self {
        Self::Div(Box::new(lhs), Box::new(rhs))
    }

    #[must_use]
    pub fn as_sea_query_expr(&self) -> sea_query::SimpleExpr {
        match self {
            Self::Field(identifier) => (*identifier).into_column_ref().into(),
            Self::Value(value) => value.to_sea_query_value().into(),
            Self::And(lhs, rhs) => lhs.as_sea_query_expr().and(rhs.as_sea_query_expr()),
            Self::Or(lhs, rhs) => lhs.as_sea_query_expr().or(rhs.as_sea_query_expr()),
            Self::Eq(lhs, rhs) => lhs.as_sea_query_expr().eq(rhs.as_sea_query_expr()),
            Self::Ne(lhs, rhs) => lhs.as_sea_query_expr().ne(rhs.as_sea_query_expr()),
            Self::Add(lhs, rhs) => lhs.as_sea_query_expr().add(rhs.as_sea_query_expr()),
            Self::Sub(lhs, rhs) => lhs.as_sea_query_expr().sub(rhs.as_sea_query_expr()),
            Self::Mul(lhs, rhs) => lhs.as_sea_query_expr().mul(rhs.as_sea_query_expr()),
            Self::Div(lhs, rhs) => lhs.as_sea_query_expr().div(rhs.as_sea_query_expr()),
        }
    }
}

/// A reference to a field in a database table.
///
/// This is used to create expressions that reference a specific column in a
/// table with a specific type. This allows for type-safe creation of queries
/// with some common operators like `=`, `!=`, `+`, `-`, `*`, and `/`.
#[derive(Debug)]
pub struct FieldRef<T> {
    identifier: Identifier,
    phantom_data: PhantomData<T>,
}

impl<T: FromDbValue + ToDbValue> FieldRef<T> {
    /// Create a new field reference.
    #[must_use]
    pub const fn new(identifier: Identifier) -> Self {
        Self {
            identifier,
            phantom_data: PhantomData,
        }
    }
}

impl<T> FieldRef<T> {
    /// Returns the field reference as an [`Expr`].
    #[must_use]
    pub fn as_expr(&self) -> Expr {
        Expr::Field(self.identifier)
    }
}

/// A trait for types that can be compared in database expressions.
pub trait ExprEq<T> {
    fn eq<V: Into<T>>(self, other: V) -> Expr;

    fn ne<V: Into<T>>(self, other: V) -> Expr;
}

impl<T: ToDbValue + 'static> ExprEq<T> for FieldRef<T> {
    fn eq<V: Into<T>>(self, other: V) -> Expr {
        Expr::eq(self.as_expr(), Expr::value(other.into()))
    }

    fn ne<V: Into<T>>(self, other: V) -> Expr {
        Expr::ne(self.as_expr(), Expr::value(other.into()))
    }
}

/// A trait for database types that can be added to each other.
pub trait ExprAdd<T> {
    fn add<V: Into<T>>(self, other: V) -> Expr;
}

/// A trait for database types that can be subtracted from each other.
pub trait ExprSub<T> {
    fn sub<V: Into<T>>(self, other: V) -> Expr;
}

/// A trait for database types that can be multiplied by each other.
pub trait ExprMul<T> {
    fn mul<V: Into<T>>(self, other: V) -> Expr;
}

/// A trait for database types that can be divided by each other.
pub trait ExprDiv<T> {
    fn div<V: Into<T>>(self, other: V) -> Expr;
}

macro_rules! impl_expr {
    ($ty:ty, $trait:ident, $method:ident) => {
        impl $trait<$ty> for FieldRef<$ty> {
            fn $method<V: Into<$ty>>(self, other: V) -> Expr {
                Expr::$method(self.as_expr(), Expr::value(other.into()))
            }
        }
    };
}

macro_rules! impl_num_expr {
    ($ty:ty) => {
        impl_expr!($ty, ExprAdd, add);
        impl_expr!($ty, ExprSub, sub);
        impl_expr!($ty, ExprMul, mul);
        impl_expr!($ty, ExprDiv, div);
    };
}

impl_num_expr!(i8);
impl_num_expr!(i16);
impl_num_expr!(i32);
impl_num_expr!(i64);
impl_num_expr!(u8);
impl_num_expr!(u16);
impl_num_expr!(u32);
impl_num_expr!(u64);
impl_num_expr!(f32);
impl_num_expr!(f64);

#[cfg(test)]
mod tests {
    use flareon_macros::model;

    use super::*;
    use crate::db::{MockDatabaseBackend, RowsNum};

    #[model]
    #[derive(std::fmt::Debug, PartialEq, Eq)]
    struct MockModel {
        id: i32,
    }

    #[test]
    fn test_new_query() {
        let query: Query<MockModel> = Query::new();

        assert!(query.filter.is_none());
    }

    #[test]
    fn test_default_query() {
        let query: Query<MockModel> = Query::default();

        assert!(query.filter.is_none());
    }

    #[test]
    fn test_query_filter() {
        let mut query: Query<MockModel> = Query::new();

        query.filter(Expr::eq(Expr::field("name"), Expr::value("John")));

        assert!(query.filter.is_some());
    }

    #[tokio::test]
    async fn test_query_all() {
        let mut db = MockDatabaseBackend::new();
        db.expect_query().returning(|_| Ok(Vec::<MockModel>::new()));
        let query: Query<MockModel> = Query::new();

        let result = query.all(&db).await;

        assert_eq!(result.unwrap(), Vec::<MockModel>::new());
    }

    #[tokio::test]
    async fn test_query_get() {
        let mut db = MockDatabaseBackend::new();
        db.expect_get().returning(|_| Ok(Option::<MockModel>::None));
        let query: Query<MockModel> = Query::new();

        let result = query.get(&db).await;

        assert_eq!(result.unwrap(), Option::<MockModel>::None);
    }

    #[tokio::test]
    async fn test_query_exists() {
        let mut db = MockDatabaseBackend::new();
        db.expect_exists()
            .returning(|_: &Query<MockModel>| Ok(false));

        let query: Query<MockModel> = Query::new();

        let result = query.exists(&db).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_query_delete() {
        let mut db = MockDatabaseBackend::new();
        db.expect_delete()
            .returning(|_: &Query<MockModel>| Ok(StatementResult::new(RowsNum(0))));
        let query: Query<MockModel> = Query::new();

        let result = query.delete(&db).await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_expr_field() {
        let expr = Expr::field("name");
        if let Expr::Field(identifier) = expr {
            assert_eq!(identifier.to_string(), "name");
        } else {
            panic!("Expected Expr::Field");
        }
    }

    #[test]
    fn test_expr_value() {
        let expr = Expr::value(30);
        if let Expr::Value(value) = expr {
            assert_eq!(value.to_sea_query_value().to_string(), "30");
        } else {
            panic!("Expected Expr::Value");
        }
    }

    #[test]
    fn test_expr_and() {
        let expr = Expr::and(Expr::field("name"), Expr::value("John"));
        if let Expr::And(lhs, rhs) = expr {
            assert!(matches!(*lhs, Expr::Field(_)));
            assert!(matches!(*rhs, Expr::Value(_)));
        } else {
            panic!("Expected Expr::And");
        }
    }

    #[test]
    fn test_expr_eq() {
        let expr = Expr::eq(Expr::field("name"), Expr::value("John"));
        if let Expr::Eq(lhs, rhs) = expr {
            assert!(matches!(*lhs, Expr::Field(_)));
            assert!(matches!(*rhs, Expr::Value(_)));
        } else {
            panic!("Expected Expr::Eq");
        }
    }
}
