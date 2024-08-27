use std::marker::PhantomData;

use derive_more::Debug;
use sea_query::IntoColumnRef;

use crate::db;
use crate::db::{Database, FromDbValue, Identifier, Model, StatementResult, ToDbValue};

#[derive(Debug)]
pub struct Query<T> {
    filter: Option<Expr>,
    phantom_data: PhantomData<T>,
}

impl<T: Model> Default for Query<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Model> Query<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            filter: None,
            phantom_data: PhantomData,
        }
    }

    pub fn filter(&mut self, filter: Expr) -> &mut Self {
        self.filter = Some(filter);
        self
    }

    pub async fn all(&self, db: &Database) -> db::Result<Vec<T>> {
        db.query(self).await
    }

    pub async fn delete(&self, db: &Database) -> db::Result<StatementResult> {
        db.delete(self).await
    }

    pub(super) fn modify_statement<S: sea_query::ConditionalStatement>(&self, statement: &mut S) {
        if let Some(filter) = &self.filter {
            statement.and_where(filter.as_sea_query_expr());
        }
    }
}

#[derive(Debug)]
pub enum Expr {
    Column(Identifier),
    Value(#[debug("{}", _0.as_sea_query_value())] Box<dyn ToDbValue>),
    Eq(Box<Expr>, Box<Expr>),
}

impl Expr {
    #[must_use]
    pub fn value<T: ToDbValue + 'static>(value: T) -> Self {
        Self::Value(Box::new(value))
    }

    #[must_use]
    pub fn eq(lhs: Self, rhs: Self) -> Self {
        Self::Eq(Box::new(lhs), Box::new(rhs))
    }

    #[must_use]
    pub fn as_sea_query_expr(&self) -> sea_query::SimpleExpr {
        match self {
            Self::Column(identifier) => identifier.clone().into_column_ref().into(),
            Self::Eq(lhs, rhs) => lhs.as_sea_query_expr().eq(rhs.as_sea_query_expr()),
            Self::Value(value) => value.as_sea_query_value().into(),
        }
    }
}

#[derive(Debug)]
pub struct FieldRef<T> {
    identifier: Identifier,
    phantom_data: PhantomData<T>,
}

impl<T: FromDbValue + ToDbValue> FieldRef<T> {
    #[must_use]
    pub const fn new(identifier: Identifier) -> Self {
        Self {
            identifier,
            phantom_data: PhantomData,
        }
    }
}

pub trait ExprEq<T> {
    fn eq(self, other: T) -> Expr;
}

impl<T: ToDbValue + 'static> ExprEq<T> for FieldRef<T> {
    fn eq(self, other: T) -> Expr {
        Expr::eq(Expr::Column(self.identifier), Expr::value(other))
    }
}
