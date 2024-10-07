use std::fmt;
use std::fmt::{Debug, Formatter};

use flareon_macros::{model, query};
use log::info;
use sea_query::ColumnDef;

use crate::db::{ColumnType, Database, DatabaseField, Identifier, Result};

/// A migration engine that can run migrations.
#[derive(Debug)]
pub struct MigrationEngine {
    migrations: Vec<MigrationWrapper>,
}

impl MigrationEngine {
    #[must_use]
    pub fn new<T: DynMigration + 'static, V: IntoIterator<Item = T>>(migrations: V) -> Self {
        let migrations = migrations.into_iter().map(MigrationWrapper::new).collect();
        Self::from_wrapper(migrations)
    }

    #[must_use]
    pub fn from_wrapper(mut migrations: Vec<MigrationWrapper>) -> Self {
        Self::sort_migrations(&mut migrations);
        Self { migrations }
    }

    /// Sorts the migrations by app name and migration name to ensure that the
    /// order of applying migrations is consistent and deterministic. Then
    /// determines the correct order of applying migrations based on the
    /// dependencies between them.
    pub fn sort_migrations<T: DynMigration>(migrations: &mut [T]) {
        migrations.sort_by(|a, b| (a.app_name(), a.name()).cmp(&(b.app_name(), b.name())));
        // TODO: Determine the correct order based on the dependencies
    }

    /// Runs the migrations. If a migration is already applied, it will be
    /// skipped.
    ///
    /// This method will also create the `flareon__migrations` table if it does
    /// not exist that is used to keep track of which migrations have been
    /// applied.
    ///
    /// # Errors
    ///
    /// Throws an error if any of the migrations fail to apply, or if there is
    /// an error while interacting with the database, or if there is an
    /// error while marking a migration as applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::db::migrations::{Field, Migration, MigrationEngine, Operation};
    /// use flareon::db::{Database, DatabaseField, Identifier};
    /// use flareon::Result;
    ///
    /// struct MyMigration;
    ///
    /// impl Migration for MyMigration {
    ///     const APP_NAME: &'static str = "todoapp";
    ///     const MIGRATION_NAME: &'static str = "m_0001_initial";
    ///     const OPERATIONS: &'static [Operation] = &[Operation::create_model()
    ///         .table_name(Identifier::new("todoapp__my_model"))
    ///         .fields(&[
    ///             Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
    ///                 .primary_key()
    ///                 .auto(),
    ///             Field::new(Identifier::new("app"), <String as DatabaseField>::TYPE),
    ///         ])
    ///         .build()];
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let engine = MigrationEngine::new([MyMigration]);
    /// let database = Database::new("sqlite::memory:").await?;
    /// engine.run(&database).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(&self, database: &Database) -> Result<()> {
        info!("Running migrations");

        CREATE_APPLIED_MIGRATIONS_MIGRATION
            .forwards(database)
            .await?;

        for migration in &self.migrations {
            for operation in migration.operations() {
                if Self::is_migration_applied(database, migration).await? {
                    info!(
                        "Migration {} for app {} is already applied",
                        migration.name(),
                        migration.app_name()
                    );
                    continue;
                }

                info!(
                    "Applying migration {} for app {}",
                    migration.name(),
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
        migration: &MigrationWrapper,
    ) -> Result<bool> {
        query!(
            AppliedMigration,
            $app == migration.app_name() && $name == migration.name()
        )
        .exists(database)
        .await
    }

    async fn mark_migration_applied(
        database: &Database,
        migration: &MigrationWrapper,
    ) -> Result<()> {
        let mut applied_migration = AppliedMigration {
            id: 0,
            app: migration.app_name().to_string(),
            name: migration.name().to_string(),
            applied: chrono::Utc::now().into(),
        };

        database.insert(&mut applied_migration).await?;
        Ok(())
    }
}

/// A migration operation that can be run forwards or backwards.
///
/// # Examples
///
/// ```
/// use flareon::db::migrations::{Field, Migration, MigrationEngine, Operation};
/// use flareon::db::{Database, DatabaseField, Identifier};
/// use flareon::Result;
///
/// # #[tokio::main]
/// # async fn main() -> Result<()> {
/// const OPERATION: Operation = Operation::create_model()
///     .table_name(Identifier::new("todoapp__my_model"))
///     .fields(&[
///         Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
///             .primary_key()
///             .auto(),
///         Field::new(Identifier::new("app"), <String as DatabaseField>::TYPE),
///     ])
///     .build();
///
/// let database = Database::new("sqlite::memory:").await?;
/// OPERATION.forwards(&database).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Copy, Clone)]
pub struct Operation {
    inner: OperationInner,
}

impl Operation {
    #[must_use]
    const fn new(inner: OperationInner) -> Self {
        Self { inner }
    }

    /// Returns a builder for an operation that creates a model.
    #[must_use]
    pub const fn create_model() -> CreateModelBuilder {
        CreateModelBuilder::new()
    }

    /// Returns a builder for an operation that adds a field to a model.
    #[must_use]
    pub const fn add_field() -> AddFieldBuilder {
        AddFieldBuilder::new()
    }

    /// Runs the operation forwards.
    ///
    /// # Errors
    ///
    /// Throws an error if the operation fails to apply.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::db::migrations::{Field, Migration, MigrationEngine, Operation};
    /// use flareon::db::{Database, DatabaseField, Identifier};
    /// use flareon::Result;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// const OPERATION: Operation = Operation::create_model()
    ///     .table_name(Identifier::new("todoapp__my_model"))
    ///     .fields(&[
    ///         Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
    ///             .primary_key()
    ///             .auto(),
    ///         Field::new(Identifier::new("app"), <String as DatabaseField>::TYPE),
    ///     ])
    ///     .build();
    ///
    /// let database = Database::new("sqlite::memory:").await?;
    /// OPERATION.forwards(&database).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn forwards(&self, database: &Database) -> Result<()> {
        match &self.inner {
            OperationInner::CreateModel {
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
            OperationInner::AddField { table_name, field } => {
                let query = sea_query::Table::alter()
                    .table(*table_name)
                    .add_column(ColumnDef::from(field))
                    .to_owned();
                database.execute_schema(query).await?;
            }
        }
        Ok(())
    }

    /// Runs the operation backwards, undoing the changes made by the forwards
    /// operation.
    ///
    /// # Errors
    ///
    /// Throws an error if the operation fails to apply.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::db::migrations::{Field, Migration, MigrationEngine, Operation};
    /// use flareon::db::{Database, DatabaseField, Identifier};
    /// use flareon::Result;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// const OPERATION: Operation = Operation::create_model()
    ///     .table_name(Identifier::new("todoapp__my_model"))
    ///     .fields(&[
    ///         Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
    ///             .primary_key()
    ///             .auto(),
    ///         Field::new(Identifier::new("app"), <String as DatabaseField>::TYPE),
    ///     ])
    ///     .build();
    ///
    /// let database = Database::new("sqlite::memory:").await?;
    /// OPERATION.forwards(&database).await?;
    /// OPERATION.backwards(&database).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn backwards(&self, database: &Database) -> Result<()> {
        match &self.inner {
            OperationInner::CreateModel {
                table_name,
                fields: _,
                if_not_exists: _,
            } => {
                let query = sea_query::Table::drop().table(*table_name).to_owned();
                database.execute_schema(query).await?;
            }
            OperationInner::AddField { table_name, field } => {
                let query = sea_query::Table::alter()
                    .table(*table_name)
                    .drop_column(field.name)
                    .to_owned();
                database.execute_schema(query).await?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
enum OperationInner {
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

#[derive(Debug, Copy, Clone)]
pub struct Field {
    /// The name of the field
    pub name: Identifier,
    /// The type of the field
    pub ty: ColumnType,
    /// Whether the column is a primary key
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
            name,
            ty,
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
        let mut def = ColumnDef::new_with_type(column.name, column.ty.into());
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
        Operation::new(OperationInner::CreateModel {
            table_name: unwrap_builder_option!(self, table_name),
            fields: unwrap_builder_option!(self, fields),
            if_not_exists: self.if_not_exists,
        })
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
        Operation::new(OperationInner::AddField {
            table_name: unwrap_builder_option!(self, table_name),
            field: unwrap_builder_option!(self, field),
        })
    }
}

pub trait Migration {
    const APP_NAME: &'static str;
    const MIGRATION_NAME: &'static str;
    const OPERATIONS: &'static [Operation];
}

pub trait DynMigration {
    fn app_name(&self) -> &str;
    fn name(&self) -> &str;
    fn operations(&self) -> &[Operation];
}

impl<T: Migration + Send + Sync + 'static> DynMigration for T {
    fn app_name(&self) -> &str {
        Self::APP_NAME
    }

    fn name(&self) -> &str {
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

    fn name(&self) -> &str {
        DynMigration::name(*self)
    }

    fn operations(&self) -> &[Operation] {
        DynMigration::operations(*self)
    }
}

impl DynMigration for Box<dyn DynMigration> {
    fn app_name(&self) -> &str {
        DynMigration::app_name(&**self)
    }

    fn name(&self) -> &str {
        DynMigration::name(&**self)
    }

    fn operations(&self) -> &[Operation] {
        DynMigration::operations(&**self)
    }
}

pub struct MigrationWrapper(Box<dyn DynMigration>);

impl MigrationWrapper {
    #[must_use]
    pub(crate) fn new<T: DynMigration + 'static>(migration: T) -> Self {
        Self(Box::new(migration))
    }
}

impl DynMigration for MigrationWrapper {
    fn app_name(&self) -> &str {
        self.0.app_name()
    }

    fn name(&self) -> &str {
        self.0.name()
    }

    fn operations(&self) -> &[Operation] {
        self.0.operations()
    }
}

impl Debug for MigrationWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynMigrationWrapper")
            .field("app_name", &self.app_name())
            .field("migration_name", &self.name())
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

const CREATE_APPLIED_MIGRATIONS_MIGRATION: Operation = Operation::create_model()
    .table_name(Identifier::new("flareon__migrations"))
    .fields(&[
        Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
            .primary_key()
            .auto(),
        Field::new(Identifier::new("app"), <String as DatabaseField>::TYPE),
        Field::new(Identifier::new("name"), <String as DatabaseField>::TYPE),
        Field::new(
            Identifier::new("applied"),
            <chrono::DateTime<chrono::FixedOffset> as DatabaseField>::TYPE,
        ),
    ])
    .if_not_exists()
    .build();
