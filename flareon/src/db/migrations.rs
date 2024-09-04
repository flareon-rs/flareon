use std::fmt;
use std::fmt::{Debug, Formatter};

use flareon_macros::{model, query};
use log::info;
use sea_query::ColumnDef;

use crate::db::{ColumnType, Database, DbField, Identifier, Result};

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

        APPLIED_MIGRATION_MIGRATION.forwards(database).await?;

        for migration in &self.migrations {
            for operation in migration.operations() {
                if Self::is_migration_applied(database, migration).await? {
                    info!(
                        "Migration {} for app {} is already applied",
                        migration.migration_name(),
                        migration.app_name()
                    );
                    continue;
                }

                info!(
                    "Applying migration {} for app {}",
                    migration.migration_name(),
                    migration.app_name()
                );
                operation.forwards(database).await?;
                Self::mark_migration_applied(database, migration).await?;
            }
        }

        Ok(())
    }

    async fn is_migration_applied(
        database: &Database,
        migration: &DynMigrationWrapper,
    ) -> Result<bool> {
        query!(
            AppliedMigration,
            $app == migration.app_name() && $name == migration.migration_name()
        )
        .exists(database)
        .await
    }

    async fn mark_migration_applied(
        database: &Database,
        migration: &DynMigrationWrapper,
    ) -> Result<()> {
        let mut applied_migration = AppliedMigration {
            id: 0,
            app: migration.app_name().to_string(),
            name: migration.migration_name().to_string(),
            applied: chrono::Utc::now().into(),
        };

        database.insert(&mut applied_migration).await?;
        Ok(())
    }
}

/// A migration operation that can be run forwards or backwards.
///
/// The preferred way to create an operation is to use the
/// `Operation::create_model`, `Operation::add_field`, etc. methods, as they
/// provide backwards compatibility in case the `Operation` struct is changed in
/// the future.
#[derive(Debug, Copy, Clone)]
pub enum Operation {
    /// Create a new model with the given fields.
    CreateModel {
        table_name: Identifier,
        fields: &'static [Field],
        if_not_exists: bool,
    },
    /// Add a new field to an existing model.
    AddField {
        table_name: Identifier,
        field: Field,
    },
}

impl Operation {
    #[must_use]
    pub const fn create_model() -> CreateModelBuilder {
        CreateModelBuilder::new()
    }

    #[must_use]
    pub const fn add_field() -> AddFieldBuilder {
        AddFieldBuilder::new()
    }

    pub async fn forwards(&self, database: &Database) -> Result<()> {
        match self {
            Self::CreateModel {
                table_name,
                fields,
                if_not_exists,
            } => {
                let mut query = sea_query::Table::create().table(*table_name).to_owned();
                for field in *fields {
                    query.col(ColumnDef::from(field));
                }
                if *if_not_exists {
                    query.if_not_exists();
                }
                database.execute_schema(query).await?;
            }
            Self::AddField { table_name, field } => {
                let query = sea_query::Table::alter()
                    .table(*table_name)
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
                if_not_exists: _,
            } => {
                let query = sea_query::Table::drop().table(*table_name).to_owned();
                database.execute_schema(query).await?;
            }
            Self::AddField { table_name, field } => {
                let query = sea_query::Table::alter()
                    .table(*table_name)
                    .drop_column(field.column_name)
                    .to_owned();
                database.execute_schema(query).await?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
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
        let mut def = ColumnDef::new_with_type(column.column_name, column.column_type.into());
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

macro_rules! unwrap_builder_option {
    ($self:ident, $field:ident) => {
        match $self.$field {
            Some(value) => value,
            None => panic!(concat!("`", stringify!($field), "` is required")),
        }
    };
}

#[derive(Debug, Copy, Clone)]
pub struct CreateModelBuilder {
    table_name: Option<Identifier>,
    fields: Option<&'static [Field]>,
    if_not_exists: bool,
}

impl Default for CreateModelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CreateModelBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            table_name: None,
            fields: None,
            if_not_exists: false,
        }
    }

    #[must_use]
    pub const fn table_name(mut self, table_name: Identifier) -> Self {
        self.table_name = Some(table_name);
        self
    }

    #[must_use]
    pub const fn fields(mut self, fields: &'static [Field]) -> Self {
        self.fields = Some(fields);
        self
    }

    #[must_use]
    pub const fn if_not_exists(mut self) -> Self {
        self.if_not_exists = true;
        self
    }

    #[must_use]
    pub const fn build(self) -> Operation {
        Operation::CreateModel {
            table_name: unwrap_builder_option!(self, table_name),
            fields: unwrap_builder_option!(self, fields),
            if_not_exists: self.if_not_exists,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct AddFieldBuilder {
    table_name: Option<Identifier>,
    field: Option<Field>,
}

impl Default for AddFieldBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AddFieldBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            table_name: None,
            field: None,
        }
    }

    #[must_use]
    pub const fn table_name(mut self, table_name: Identifier) -> Self {
        self.table_name = Some(table_name);
        self
    }

    #[must_use]
    pub const fn field(mut self, field: Field) -> Self {
        self.field = Some(field);
        self
    }

    #[must_use]
    pub const fn build(self) -> Operation {
        Operation::AddField {
            table_name: unwrap_builder_option!(self, table_name),
            field: unwrap_builder_option!(self, field),
        }
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

#[derive(Debug)]
#[model(table_name = "flareon__migrations", model_type = "internal")]
struct AppliedMigration {
    id: i32,
    app: String,
    name: String,
    applied: chrono::DateTime<chrono::FixedOffset>,
}

const APPLIED_MIGRATION_MIGRATION: Operation = Operation::create_model()
    .table_name(Identifier::new("flareon__migrations"))
    .fields(&[
        Field::new(Identifier::new("id"), <i32 as DbField>::TYPE)
            .primary_key()
            .auto(),
        Field::new(Identifier::new("app"), <String as DbField>::TYPE),
        Field::new(Identifier::new("name"), <String as DbField>::TYPE),
        Field::new(
            Identifier::new("applied"),
            <chrono::DateTime<chrono::FixedOffset> as DbField>::TYPE,
        ),
    ])
    .if_not_exists()
    .build();
