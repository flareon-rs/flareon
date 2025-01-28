use std::marker::PhantomData;

use derive_more::Debug;
use sea_query::{ExprTrait, IntoColumnRef};

use crate::db;
use crate::db::{
    Auto, DatabaseBackend, DbFieldValue, DbValue, ForeignKey, FromDbValue, Identifier, Model,
    StatementResult, ToDbFieldValue,
};

/// A query that can be executed on a database. Can be used to filter, update,
/// or delete rows.
///
/// # Example
/// ```
/// use cot::db::model;
/// use cot::db::query::Query;
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
pub struct Query<T> {
    filter: Option<Expr>,
    phantom_data: PhantomData<fn() -> T>,
}

// manual implementation to avoid `T: Debug` in the trait bounds
impl<T> Debug for Query<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("filter", &self.filter)
            .field("phantom_data", &self.phantom_data)
            .finish()
    }
}

// manual implementation to avoid `T: Clone` in the trait bounds
impl<T> Clone for Query<T> {
    fn clone(&self) -> Self {
        Self {
            filter: self.filter.clone(),
            phantom_data: PhantomData,
        }
    }
}

// manual implementation to avoid `T: PartialEq` in the trait bounds
impl<T> PartialEq for Query<T> {
    fn eq(&self, other: &Self) -> bool {
        self.filter == other.filter
    }
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
    /// use cot::db::model;
    /// use cot::db::query::Query;
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
    /// use cot::db::model;
    /// use cot::db::query::{Expr, Query};
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

/// An expression that can be used to filter, update, or delete rows.
///
/// This is used to create complex queries with multiple conditions. Typically,
/// it is only internally used by the [`cot::query`] macro to create a
/// [`Query`].
///
/// # Example
///
/// ```
/// use cot::db::{model, query};
/// use cot::db::query::{Expr, Query};
///
/// #[model]
/// struct MyModel {
///     #[model(primary_key)]
///     id: i32,
/// };
///
/// let expr = Expr::eq(Expr::field("id"), Expr::value(5));
///
/// assert_eq!(
///     <Query<MyModel>>::new().filter(expr),
///     query!(MyModel, $id == 5)
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// An expression containing a reference to a column.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == 5)
    /// );
    /// ```
    Field(Identifier),
    /// An expression containing a literal value.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::ne(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id != 5)
    /// );
    /// ```
    Value(DbValue),
    /// An `AND` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::and(
    ///     Expr::gt(Expr::field("id"), Expr::value(10)),
    ///     Expr::lt(Expr::field("id"), Expr::value(20))
    /// );
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id > 10 && $id < 20)
    /// );
    /// ```
    And(Box<Expr>, Box<Expr>),
    /// An `OR` expression.
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::or(
    ///     Expr::gt(Expr::field("id"), Expr::value(10)),
    ///     Expr::lt(Expr::field("id"), Expr::value(20))
    /// );
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id > 10 || $id < 20)
    /// );
    /// ```
    Or(Box<Expr>, Box<Expr>),
    /// An `=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == 5)
    /// );
    /// ```
    Eq(Box<Expr>, Box<Expr>),
    /// A `!=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::ne(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id != 5)
    /// );
    /// ```
    Ne(Box<Expr>, Box<Expr>),
    /// A `<` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::lt(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id < 5)
    /// );
    /// ```
    Lt(Box<Expr>, Box<Expr>),
    /// A `<=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::lte(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id <= 5)
    /// );
    /// ```
    Lte(Box<Expr>, Box<Expr>),
    /// A `>` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::gt(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id > 5)
    /// );
    /// ```
    Gt(Box<Expr>, Box<Expr>),
    /// A `>=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::gte(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id >= 5)
    /// );
    /// ```
    Gte(Box<Expr>, Box<Expr>),
    /// A `+` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::add(Expr::field("id_2"), Expr::value(5)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 + 5)
    /// );
    /// ```
    Add(Box<Expr>, Box<Expr>),
    /// A `-` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::sub(Expr::field("id_2"), Expr::value(5)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 - 5)
    /// );
    /// ```
    Sub(Box<Expr>, Box<Expr>),
    /// A `*` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::mul(Expr::field("id_2"), Expr::value(2)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 * 2)
    /// );
    /// ```
    Mul(Box<Expr>, Box<Expr>),
    /// A `/` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::div(Expr::field("id_2"), Expr::value(2)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 / 2)
    /// );
    /// ```
    Div(Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Create a new field expression. This represents a reference to a column
    /// in the database.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == 5)
    /// );
    /// ```
    #[must_use]
    pub fn field<T: Into<Identifier>>(identifier: T) -> Self {
        Self::Field(identifier.into())
    }

    /// Create a new value expression. This represents a literal value that gets
    /// passed into the SQL query.
    ///
    /// # Panics
    ///
    /// If the value provided is a [`DbFieldValue::Auto`].
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::ne(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id != 5)
    /// );
    /// ```
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn value<T: ToDbFieldValue>(value: T) -> Self {
        match value.to_db_field_value() {
            DbFieldValue::Value(value) => Self::Value(value),
            DbFieldValue::Auto => panic!("Cannot create query with a non-value field"),
        }
    }

    /// Create a new `AND` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::and(
    ///     Expr::gt(Expr::field("id"), Expr::value(10)),
    ///     Expr::lt(Expr::field("id"), Expr::value(20))
    /// );
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id > 10 && $id < 20)
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
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::or(
    ///     Expr::gt(Expr::field("id"), Expr::value(10)),
    ///     Expr::lt(Expr::field("id"), Expr::value(20))
    /// );
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id > 10 || $id < 20)
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
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == 5)
    /// );
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
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::ne(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id != 5)
    /// );
    /// ```
    #[must_use]
    pub fn ne(lhs: Self, rhs: Self) -> Self {
        Self::Ne(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `<` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::lt(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id < 5)
    /// );
    /// ```
    #[must_use]
    pub fn lt(lhs: Self, rhs: Self) -> Self {
        Self::Lt(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `<=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::lte(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id <= 5)
    /// );
    /// ```
    #[must_use]
    pub fn lte(lhs: Self, rhs: Self) -> Self {
        Self::Lte(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `>` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::gt(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id > 5)
    /// );
    /// ```
    #[must_use]
    pub fn gt(lhs: Self, rhs: Self) -> Self {
        Self::Gt(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `>=` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    /// };
    ///
    /// let expr = Expr::gte(Expr::field("id"), Expr::value(5));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id >= 5)
    /// );
    /// ```
    #[must_use]
    pub fn gte(lhs: Self, rhs: Self) -> Self {
        Self::Gte(Box::new(lhs), Box::new(rhs))
    }

    /// Create a new `+` expression.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::add(Expr::field("id_2"), Expr::value(5)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 + 5)
    /// );
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
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::sub(Expr::field("id_2"), Expr::value(5)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 - 5)
    /// );
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
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::mul(Expr::field("id_2"), Expr::value(2)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 * 2)
    /// );
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
    /// use cot::db::{model, query};
    /// use cot::db::query::{Expr, Query};
    ///
    /// #[model]
    /// struct MyModel {
    ///     #[model(primary_key)]
    ///     id: i32,
    ///     id_2: i32,
    /// };
    ///
    /// let expr = Expr::eq(Expr::field("id"), Expr::div(Expr::field("id_2"), Expr::value(2)));
    ///
    /// assert_eq!(
    ///     <Query<MyModel>>::new().filter(expr),
    ///     query!(MyModel, $id == $id_2 / 2)
    /// );
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
            Self::Value(value) => (*value).clone().into(),
            Self::And(lhs, rhs) => lhs.as_sea_query_expr().and(rhs.as_sea_query_expr()),
            Self::Or(lhs, rhs) => lhs.as_sea_query_expr().or(rhs.as_sea_query_expr()),
            Self::Eq(lhs, rhs) => lhs.as_sea_query_expr().eq(rhs.as_sea_query_expr()),
            Self::Ne(lhs, rhs) => lhs.as_sea_query_expr().ne(rhs.as_sea_query_expr()),
            Self::Lt(lhs, rhs) => lhs.as_sea_query_expr().lt(rhs.as_sea_query_expr()),
            Self::Lte(lhs, rhs) => lhs.as_sea_query_expr().lte(rhs.as_sea_query_expr()),
            Self::Gt(lhs, rhs) => lhs.as_sea_query_expr().gt(rhs.as_sea_query_expr()),
            Self::Gte(lhs, rhs) => lhs.as_sea_query_expr().gte(rhs.as_sea_query_expr()),
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

impl<T: FromDbValue + ToDbFieldValue> FieldRef<T> {
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
    fn eq<V: IntoField<T>>(self, other: V) -> Expr;

    fn ne<V: IntoField<T>>(self, other: V) -> Expr;
}

impl<T: ToDbFieldValue + 'static> ExprEq<T> for FieldRef<T> {
    fn eq<V: IntoField<T>>(self, other: V) -> Expr {
        Expr::eq(self.as_expr(), Expr::value(other.into_field()))
    }

    fn ne<V: IntoField<T>>(self, other: V) -> Expr {
        Expr::ne(self.as_expr(), Expr::value(other.into_field()))
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

/// A trait for database types that can be ordered.
pub trait ExprOrd<T> {
    fn lt<V: Into<T>>(self, other: V) -> Expr;

    fn lte<V: Into<T>>(self, other: V) -> Expr;

    fn gt<V: Into<T>>(self, other: V) -> Expr;

    fn gte<V: Into<T>>(self, other: V) -> Expr;
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

        impl ExprOrd<$ty> for FieldRef<$ty> {
            fn lt<V: Into<$ty>>(self, other: V) -> Expr {
                Expr::lt(self.as_expr(), Expr::value(other.into()))
            }

            fn lte<V: Into<$ty>>(self, other: V) -> Expr {
                Expr::lte(self.as_expr(), Expr::value(other.into()))
            }

            fn gt<V: Into<$ty>>(self, other: V) -> Expr {
                Expr::gt(self.as_expr(), Expr::value(other.into()))
            }

            fn gte<V: Into<$ty>>(self, other: V) -> Expr {
                Expr::gte(self.as_expr(), Expr::value(other.into()))
            }
        }
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

pub trait IntoField<T> {
    fn into_field(self) -> T;
}

impl<T: ToDbFieldValue> IntoField<T> for T {
    fn into_field(self) -> T {
        self
    }
}

impl<T> IntoField<Auto<T>> for T {
    fn into_field(self) -> Auto<T> {
        Auto::fixed(self)
    }
}

impl IntoField<String> for &str {
    fn into_field(self) -> String {
        self.to_string()
    }
}

impl<T: Model + Send + Sync> IntoField<ForeignKey<T>> for T {
    fn into_field(self) -> ForeignKey<T> {
        ForeignKey::from(self)
    }
}

impl<T: Model + Send + Sync> IntoField<ForeignKey<T>> for &T {
    fn into_field(self) -> ForeignKey<T> {
        ForeignKey::from(self)
    }
}

#[cfg(test)]
mod tests {
    use cot_macros::model;

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
            assert_eq!(value.to_string(), "30");
        } else {
            panic!("Expected Expr::Value");
        }
    }

    macro_rules! test_expr_constructor {
        ($test_name:ident, $match:ident, $constructor:ident) => {
            #[test]
            fn $test_name() {
                let expr = Expr::$constructor(Expr::field("name"), Expr::value("John"));
                if let Expr::$match(lhs, rhs) = expr {
                    assert!(matches!(*lhs, Expr::Field(_)));
                    assert!(matches!(*rhs, Expr::Value(_)));
                } else {
                    panic!(concat!("Expected Expr::", stringify!($match)));
                }
            }
        };
    }

    test_expr_constructor!(expr_and, And, and);
    test_expr_constructor!(expr_or, Or, or);
    test_expr_constructor!(expr_eq, Eq, eq);
    test_expr_constructor!(expr_ne, Ne, ne);
    test_expr_constructor!(expr_lt, Lt, lt);
    test_expr_constructor!(expr_lte, Lte, lte);
    test_expr_constructor!(expr_gt, Gt, gt);
    test_expr_constructor!(expr_gte, Gte, gte);
    test_expr_constructor!(expr_add, Add, add);
    test_expr_constructor!(expr_sub, Sub, sub);
    test_expr_constructor!(expr_mul, Mul, mul);
    test_expr_constructor!(expr_div, Div, div);
}
