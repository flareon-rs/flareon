use flareon::db::DatabaseError;

use crate::db::{DatabaseBackend, Model, Result};

/// A foreign key to another model.
///
/// Internally, this is represented either as a primary key (in case the
/// model has not been retrieved from the database) or as the model itself.
#[derive(Debug, Clone)]
pub enum ForeignKey<T: Model> {
    /// The primary key of the referenced model; used when the model has not
    /// been retrieved from the database yet or when it's unnecessary to
    /// store the entire model instance.
    PrimaryKey(T::PrimaryKey),
    /// The referenced model.
    Model(Box<T>),
}

impl<T: Model> ForeignKey<T> {
    pub fn primary_key(&self) -> &T::PrimaryKey {
        match self {
            Self::PrimaryKey(pk) => pk,
            Self::Model(model) => model.primary_key(),
        }
    }

    pub fn model(&self) -> Option<&T> {
        match self {
            Self::Model(model) => Some(model),
            Self::PrimaryKey(_) => None,
        }
    }

    pub fn unwrap(self) -> T {
        match self {
            Self::Model(model) => *model,
            Self::PrimaryKey(_) => panic!("object has not been retrieved from the database"),
        }
    }

    /// Retrieve the model from the database, if needed, and return it.
    pub async fn get<DB: DatabaseBackend>(&mut self, db: &DB) -> Result<&T> {
        match self {
            Self::Model(model) => Ok(model),
            Self::PrimaryKey(pk) => {
                let model = T::get_by_primary_key(db, pk.clone())
                    .await?
                    .ok_or(DatabaseError::ForeignKeyNotFound)?;
                *self = Self::Model(Box::new(model));
                Ok(self.model().expect("model was just set"))
            }
        }
    }
}

impl<T: Model> PartialEq for ForeignKey<T>
where
    T::PrimaryKey: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.primary_key() == other.primary_key()
    }
}

impl<T: Model> Eq for ForeignKey<T> where T::PrimaryKey: Eq {}

impl<T: Model> From<T> for ForeignKey<T> {
    fn from(model: T) -> Self {
        Self::Model(Box::new(model))
    }
}

impl<T: Model> From<&T> for ForeignKey<T> {
    fn from(model: &T) -> Self {
        Self::PrimaryKey(model.primary_key().clone())
    }
}

/// A foreign key on delete constraint.
///
/// This is used to define the behavior of a foreign key when the referenced row
/// is deleted.
///
/// # See also
///
/// - [`ForeignKeyOnUpdatePolicy`]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum ForeignKeyOnDeletePolicy {
    NoAction,
    #[default]
    Restrict,
    Cascade,
    SetNone,
}

impl From<ForeignKeyOnDeletePolicy> for sea_query::ForeignKeyAction {
    fn from(value: ForeignKeyOnDeletePolicy) -> Self {
        match value {
            ForeignKeyOnDeletePolicy::NoAction => Self::NoAction,
            ForeignKeyOnDeletePolicy::Restrict => Self::Restrict,
            ForeignKeyOnDeletePolicy::Cascade => Self::Cascade,
            ForeignKeyOnDeletePolicy::SetNone => Self::SetNull,
        }
    }
}

/// A foreign key on update constraint.
///
/// This is used to define the behavior of a foreign key when the referenced row
/// is updated.
///
/// # See also
///
/// - [`ForeignKeyOnDeletePolicy`]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum ForeignKeyOnUpdatePolicy {
    NoAction,
    Restrict,
    #[default]
    Cascade,
    SetNone,
}

impl From<ForeignKeyOnUpdatePolicy> for sea_query::ForeignKeyAction {
    fn from(value: ForeignKeyOnUpdatePolicy) -> Self {
        match value {
            ForeignKeyOnUpdatePolicy::NoAction => Self::NoAction,
            ForeignKeyOnUpdatePolicy::Restrict => Self::Restrict,
            ForeignKeyOnUpdatePolicy::Cascade => Self::Cascade,
            ForeignKeyOnUpdatePolicy::SetNone => Self::SetNull,
        }
    }
}
