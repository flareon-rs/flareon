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

    pub async fn exists(&self, db: &Database) -> db::Result<bool> {
        db.exists(self).await
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
    Field(Identifier),
    Value(#[debug("{}", _0.as_sea_query_value())] Box<dyn ToDbValue>),
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
    #[must_use]
    pub fn field<T: Into<Identifier>>(identifier: T) -> Self {
        Self::Field(identifier.into())
    }

    #[must_use]
    pub fn value<T: ToDbValue + 'static>(value: T) -> Self {
        Self::Value(Box::new(value))
    }

    #[must_use]
    pub fn and(lhs: Self, rhs: Self) -> Self {
        Self::And(Box::new(lhs), Box::new(rhs))
    }

    #[must_use]
    pub fn or(lhs: Self, rhs: Self) -> Self {
        Self::Or(Box::new(lhs), Box::new(rhs))
    }

    #[must_use]
    pub fn eq(lhs: Self, rhs: Self) -> Self {
        Self::Eq(Box::new(lhs), Box::new(rhs))
    }

    #[must_use]
    pub fn ne(lhs: Self, rhs: Self) -> Self {
        Self::Ne(Box::new(lhs), Box::new(rhs))
    }

    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn add(lhs: Self, rhs: Self) -> Self {
        Self::Add(Box::new(lhs), Box::new(rhs))
    }

    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn sub(lhs: Self, rhs: Self) -> Self {
        Self::Sub(Box::new(lhs), Box::new(rhs))
    }

    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn mul(lhs: Self, rhs: Self) -> Self {
        Self::Mul(Box::new(lhs), Box::new(rhs))
    }

    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn div(lhs: Self, rhs: Self) -> Self {
        Self::Div(Box::new(lhs), Box::new(rhs))
    }

    #[must_use]
    pub fn as_sea_query_expr(&self) -> sea_query::SimpleExpr {
        match self {
            Self::Field(identifier) => (*identifier).into_column_ref().into(),
            Self::Value(value) => value.as_sea_query_value().into(),
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

impl<T> FieldRef<T> {
    #[must_use]
    pub fn as_expr(&self) -> Expr {
        Expr::Field(self.identifier)
    }
}

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

pub trait ExprAdd<T> {
    fn add<V: Into<T>>(self, other: V) -> Expr;
}

pub trait ExprSub<T> {
    fn sub<V: Into<T>>(self, other: V) -> Expr;
}

pub trait ExprMul<T> {
    fn mul<V: Into<T>>(self, other: V) -> Expr;
}

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
