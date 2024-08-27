use std::fmt;
use std::fmt::{Debug, Formatter};

use log::info;
use sea_query::ColumnDef;

use crate::db::{ColumnType, Database, Identifier, Result};

#[derive(Debug)]
pub struct MigrationEngine {
    migrations: Vec<DynMigrationWrapper>,
}

impl MigrationEngine {
    #[must_use]
    pub fn new<T: DynMigration + 'static, V: Into<Vec<T>>>(migrations: V) -> Self {
        let mut migrations = migrations.into();
        Self::sort_migrations(&mut migrations);
        let migrations = migrations
            .into_iter()
            .map(DynMigrationWrapper::new)
            .collect();
        Self { migrations }
    }

    /// Sorts the migrations by app name and migration name to ensure that the
    /// order of applying migrations is consistent and deterministic. Then
    /// determines the correct order of applying migrations based on the
    /// dependencies between them.
    pub fn sort_migrations<T: DynMigration>(migrations: &mut [T]) {
        migrations.sort_by(|a, b| {
            (a.app_name(), a.migration_name()).cmp(&(b.app_name(), b.migration_name()))
        });
        // TODO: Determine the correct order based on the dependencies
    }

    pub async fn run(&self, database: &Database) -> Result<()> {
        info!("Running migrations");
        for migration in &self.migrations {
            info!(
                "Applying migration {} for app {}",
                migration.migration_name(),
                migration.app_name()
            );
            for operation in migration.operations() {
                operation.forwards(database).await?;
            }
        }

        Ok(())
    }
}

/// A migration operation that can be run forwards or backwards.
#[derive(Debug, Clone)]
pub enum Operation {
    /// Create a new model with the given fields.
    CreateModel {
        table_name: Identifier,
        fields: &'static [Field],
    },
    /// Add a new field to an existing model.
    AddField {
        table_name: Identifier,
        field: Field,
    },
}

impl Operation {
    pub async fn forwards(&self, database: &Database) -> Result<()> {
        match self {
            Self::CreateModel { table_name, fields } => {
                let mut query = sea_query::Table::create()
                    .table(table_name.clone())
                    .to_owned();
                for field in *fields {
                    query.col(ColumnDef::from(field));
                }
                database.execute_schema(query).await?;
            }
            Self::AddField { table_name, field } => {
                let query = sea_query::Table::alter()
                    .table(table_name.clone())
                    .add_column(ColumnDef::from(field))
                    .to_owned();
                database.execute_schema(query).await?;
            }
        }
        Ok(())
    }

    pub async fn backwards(&self, database: &Database) -> Result<()> {
        match self {
            Self::CreateModel {
                table_name,
                fields: _,
            } => {
                let query = sea_query::Table::drop()
                    .table(table_name.clone())
                    .to_owned();
                database.execute_schema(query).await?;
            }
            Self::AddField { table_name, field } => {
                let query = sea_query::Table::alter()
                    .table(table_name.clone())
                    .drop_column(field.column_name.clone())
                    .to_owned();
                database.execute_schema(query).await?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub column_name: Identifier,
    pub column_type: ColumnType,
    pub primary_key: bool,
    /// Whether the column is an auto-incrementing value (usually used as a
    /// primary key)
    pub auto_value: bool,
    /// Whether the column can be null
    pub null: bool,
}

impl Field {
    #[must_use]
    pub const fn new(name: Identifier, ty: ColumnType) -> Self {
        Self {
            column_name: name,
            column_type: ty,
            primary_key: false,
            auto_value: false,
            null: false,
        }
    }

    #[must_use]
    pub const fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self
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

impl From<&Field> for ColumnDef {
    fn from(column: &Field) -> Self {
        let mut def =
            ColumnDef::new_with_type(column.column_name.clone(), column.column_type.into());
        if column.primary_key {
            def.primary_key();
        }
        if column.auto_value {
            def.auto_increment();
        }
        if column.null {
            def.null();
        }
        def
    }
}

pub trait Migration {
    const APP_NAME: &'static str;
    const MIGRATION_NAME: &'static str;
    const OPERATIONS: &'static [Operation];
}

pub trait DynMigration {
    fn app_name(&self) -> &str;
    fn migration_name(&self) -> &str;
    fn operations(&self) -> &[Operation];
}

impl<T: Migration> DynMigration for T {
    fn app_name(&self) -> &str {
        Self::APP_NAME
    }

    fn migration_name(&self) -> &str {
        Self::MIGRATION_NAME
    }

    fn operations(&self) -> &[Operation] {
        Self::OPERATIONS
    }
}

impl DynMigration for &dyn DynMigration {
    fn app_name(&self) -> &str {
        DynMigration::app_name(*self)
    }

    fn migration_name(&self) -> &str {
        DynMigration::migration_name(*self)
    }

    fn operations(&self) -> &[Operation] {
        DynMigration::operations(*self)
    }
}

struct DynMigrationWrapper(Box<dyn DynMigration>);

impl DynMigrationWrapper {
    #[must_use]
    fn new<T: DynMigration + 'static>(migration: T) -> Self {
        Self(Box::new(migration))
    }
}

impl DynMigration for DynMigrationWrapper {
    fn app_name(&self) -> &str {
        self.0.app_name()
    }

    fn migration_name(&self) -> &str {
        self.0.migration_name()
    }

    fn operations(&self) -> &[Operation] {
        self.0.operations()
    }
}

impl Debug for DynMigrationWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynMigrationWrapper")
            .field("app_name", &self.app_name())
            .field("migration_name", &self.migration_name())
            .field("operations", &self.operations())
            .finish()
    }
}

impl From<ColumnType> for sea_query::ColumnType {
    fn from(value: ColumnType) -> Self {
        match value {
            ColumnType::Boolean => Self::Boolean,
            ColumnType::TinyInteger => Self::TinyInteger,
            ColumnType::SmallInteger => Self::SmallInteger,
            ColumnType::Integer => Self::Integer,
            ColumnType::BigInteger => Self::BigInteger,
            ColumnType::TinyUnsignedInteger => Self::TinyUnsigned,
            ColumnType::SmallUnsignedInteger => Self::SmallUnsigned,
            ColumnType::UnsignedInteger => Self::Unsigned,
            ColumnType::BigUnsignedInteger => Self::BigUnsigned,
            ColumnType::Float => Self::Float,
            ColumnType::Double => Self::Double,
            ColumnType::Time => Self::Time,
            ColumnType::Date => Self::Date,
            ColumnType::DateTime => Self::DateTime,
            ColumnType::Timestamp => Self::Timestamp,
            ColumnType::TimestampWithTimeZone => Self::TimestampWithTimeZone,
            ColumnType::Text => Self::Text,
        }
    }
}
