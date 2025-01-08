mod sorter;

use std::fmt;
use std::fmt::{Debug, Formatter};

use flareon::db::relations::ForeignKeyOnUpdatePolicy;
use sea_query::{ColumnDef, StringLen};
use thiserror::Error;
use tracing::info;

use crate::db::migrations::sorter::{MigrationSorter, MigrationSorterError};
use crate::db::relations::ForeignKeyOnDeletePolicy;
use crate::db::{model, query, ColumnType, Database, DatabaseField, Identifier, Result};

#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum MigrationEngineError {
    /// An error occurred while determining the correct order of migrations.
    #[error("Error while determining the correct order of migrations")]
    MigrationSortError(#[from] MigrationSorterError),
}

/// A migration engine that can run migrations.
#[derive(Debug)]
pub struct MigrationEngine {
    migrations: Vec<MigrationWrapper>,
}

impl MigrationEngine {
    pub fn new<T: DynMigration + 'static, V: IntoIterator<Item = T>>(
        migrations: V,
    ) -> Result<Self> {
        let migrations = migrations.into_iter().map(MigrationWrapper::new).collect();
        Self::from_wrapper(migrations)
    }

    pub fn from_wrapper(mut migrations: Vec<MigrationWrapper>) -> Result<Self> {
        Self::sort_migrations(&mut migrations)?;
        Ok(Self { migrations })
    }

    /// Sorts the migrations by app name and migration name to ensure that the
    /// order of applying migrations is consistent and deterministic. Then
    /// determines the correct order of applying migrations based on the
    /// dependencies between them.
    pub fn sort_migrations<T: DynMigration>(migrations: &mut [T]) -> Result<()> {
        MigrationSorter::new(migrations)
            .sort()
            .map_err(MigrationEngineError::from)?;
        Ok(())
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
    /// use flareon::db::migrations::{
    ///     Field, Migration, MigrationDependency, MigrationEngine, Operation,
    /// };
    /// use flareon::db::{Database, DatabaseField, Identifier};
    /// use flareon::Result;
    ///
    /// struct MyMigration;
    ///
    /// impl Migration for MyMigration {
    ///     const APP_NAME: &'static str = "todoapp";
    ///     const MIGRATION_NAME: &'static str = "m_0001_initial";
    ///     const DEPENDENCIES: &'static [MigrationDependency] = &[];
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
    /// let engine = MigrationEngine::new([MyMigration])?;
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
                    query.col(field.as_column_def(database));
                    if let Some(foreign_key) = field.foreign_key {
                        query.foreign_key(
                            sea_query::ForeignKeyCreateStatement::new()
                                .from_tbl(*table_name)
                                .from_col(field.name)
                                .to_tbl(foreign_key.model)
                                .to_col(foreign_key.field)
                                .on_delete(foreign_key.on_delete.into())
                                .on_update(foreign_key.on_update.into()),
                        );
                    }
                }
                if *if_not_exists {
                    query.if_not_exists();
                }
                database.execute_schema(query).await?;
            }
            OperationInner::AddField { table_name, field } => {
                let query = sea_query::Table::alter()
                    .table(*table_name)
                    .add_column(field.as_column_def(database))
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
    /// Whether the column has a unique constraint
    pub unique: bool,
    foreign_key: Option<ForeignKeyReference>,
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
            unique: false,
            foreign_key: None,
        }
    }

    #[must_use]
    pub const fn foreign_key(
        mut self,
        to_model: Identifier,
        to_field: Identifier,
        on_delete: ForeignKeyOnDeletePolicy,
        on_update: ForeignKeyOnUpdatePolicy,
    ) -> Self {
        assert!(
            self.null || !matches!(on_delete, ForeignKeyOnDeletePolicy::SetNone),
            "`ForeignKey` must be inside `Option` if `on_delete` is set to `SetNone`"
        );
        assert!(
            self.null || !matches!(on_update, ForeignKeyOnUpdatePolicy::SetNone),
            "`ForeignKey` must be inside `Option` if `on_update` is set to `SetNone`"
        );

        self.foreign_key = Some(ForeignKeyReference {
            model: to_model,
            field: to_field,
            on_delete,
            on_update,
        });
        self
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

    #[must_use]
    pub const fn set_null(mut self, value: bool) -> Self {
        self.null = value;
        self
    }

    #[must_use]
    pub const fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    fn as_column_def<T: ColumnTypeMapper>(&self, mapper: &T) -> ColumnDef {
        let mut def =
            ColumnDef::new_with_type(self.name, mapper.sea_query_column_type_for(self.ty));
        if self.primary_key {
            def.primary_key();
        }
        if self.auto_value {
            def.auto_increment();
        }
        if self.null {
            def.null();
        } else {
            def.not_null();
        }
        if self.unique {
            def.unique_key();
        }
        def
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ForeignKeyReference {
    model: Identifier,
    field: Identifier,
    on_delete: ForeignKeyOnDeletePolicy,
    on_update: ForeignKeyOnUpdatePolicy,
}

#[cfg_attr(test, mockall::automock)]
pub(super) trait ColumnTypeMapper {
    fn sea_query_column_type_for(&self, column_type: ColumnType) -> sea_query::ColumnType;
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
    const DEPENDENCIES: &'static [MigrationDependency];
    const OPERATIONS: &'static [Operation];
}

pub trait DynMigration {
    fn app_name(&self) -> &str;
    fn name(&self) -> &str;
    fn dependencies(&self) -> &[MigrationDependency];
    fn operations(&self) -> &[Operation];
}

impl<T: Migration + Send + Sync + 'static> DynMigration for T {
    fn app_name(&self) -> &str {
        Self::APP_NAME
    }

    fn name(&self) -> &str {
        Self::MIGRATION_NAME
    }

    fn dependencies(&self) -> &[MigrationDependency] {
        Self::DEPENDENCIES
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

    fn dependencies(&self) -> &[MigrationDependency] {
        DynMigration::dependencies(*self)
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

    fn dependencies(&self) -> &[MigrationDependency] {
        DynMigration::dependencies(&**self)
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

    fn dependencies(&self) -> &[MigrationDependency] {
        self.0.dependencies()
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
            ColumnType::DateTimeWithTimeZone => Self::TimestampWithTimeZone,
            ColumnType::Text => Self::Text,
            ColumnType::Blob => Self::Blob,
            ColumnType::String(len) => Self::String(StringLen::N(len)),
        }
    }
}

/// A migration dependency: a relationship between two migrations that tells the
/// migration engine which migrations need to be applied before
/// others.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct MigrationDependency {
    inner: MigrationDependencyInner,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum MigrationDependencyInner {
    Migration {
        app: &'static str,
        migration: &'static str,
    },
    Model {
        app: &'static str,
        table_name: &'static str,
    },
}

impl MigrationDependency {
    #[must_use]
    const fn new(inner: MigrationDependencyInner) -> Self {
        Self { inner }
    }

    /// Creates a dependency on another migration.
    ///
    /// This ensures that the migration engine will apply the migration with
    /// given app and migration name before the current migration.
    #[must_use]
    pub const fn migration(app: &'static str, migration: &'static str) -> Self {
        Self::new(MigrationDependencyInner::Migration { app, migration })
    }

    /// Creates a dependency on a model.
    ///
    /// This ensures that the migration engine will apply the migration that
    /// creates the model with the given app and table name before the current
    /// migration.
    #[must_use]
    pub const fn model(app: &'static str, table_name: &'static str) -> Self {
        Self::new(MigrationDependencyInner::Model { app, table_name })
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

#[cfg(test)]
mod tests {
    use flareon::test::TestDatabase;
    use sea_query::ColumnSpec;

    use super::*;
    use crate::db::{ColumnType, DatabaseField, Identifier};

    struct TestMigration;

    impl Migration for TestMigration {
        const APP_NAME: &'static str = "testapp";
        const MIGRATION_NAME: &'static str = "m_0001_initial";
        const DEPENDENCIES: &'static [MigrationDependency] = &[];
        const OPERATIONS: &'static [Operation] = &[Operation::create_model()
            .table_name(Identifier::new("testapp__test_model"))
            .fields(&[
                Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
                    .primary_key()
                    .auto(),
                Field::new(Identifier::new("name"), <String as DatabaseField>::TYPE),
            ])
            .build()];
    }

    #[flareon_macros::dbtest]
    async fn test_migration_engine_run(test_db: &mut TestDatabase) {
        let engine = MigrationEngine::new([TestMigration]).unwrap();

        let result = engine.run(&test_db.database()).await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_operation_create_model() {
        const OPERATION_CREATE_MODEL_FIELDS: &[Field; 2] = &[
            Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
                .primary_key()
                .auto(),
            Field::new(Identifier::new("name"), <String as DatabaseField>::TYPE),
        ];

        let operation = Operation::create_model()
            .table_name(Identifier::new("testapp__test_model"))
            .fields(OPERATION_CREATE_MODEL_FIELDS)
            .build();

        if let OperationInner::CreateModel {
            table_name,
            fields,
            if_not_exists,
        } = operation.inner
        {
            assert_eq!(table_name.to_string(), "testapp__test_model");
            assert_eq!(fields.len(), 2);
            assert!(!if_not_exists);
        } else {
            panic!("Expected OperationInner::CreateModel");
        }
    }

    #[test]
    fn test_operation_add_field() {
        let operation = Operation::add_field()
            .table_name(Identifier::new("testapp__test_model"))
            .field(Field::new(
                Identifier::new("age"),
                <i32 as DatabaseField>::TYPE,
            ))
            .build();

        if let OperationInner::AddField { table_name, field } = operation.inner {
            assert_eq!(table_name.to_string(), "testapp__test_model");
            assert_eq!(field.name.to_string(), "age");
        } else {
            panic!("Expected OperationInner::AddField");
        }
    }

    #[test]
    fn field_new() {
        let field = Field::new(Identifier::new("id"), ColumnType::Integer)
            .primary_key()
            .auto()
            .null();

        assert_eq!(field.name.to_string(), "id");
        assert_eq!(field.ty, ColumnType::Integer);
        assert!(field.primary_key);
        assert!(field.auto_value);
        assert!(field.null);
    }

    #[test]
    fn field_foreign_key() {
        let field = Field::new(Identifier::new("parent"), ColumnType::Integer).foreign_key(
            Identifier::new("testapp__parent"),
            Identifier::new("id"),
            ForeignKeyOnDeletePolicy::Restrict,
            ForeignKeyOnUpdatePolicy::Restrict,
        );

        assert_eq!(
            field.foreign_key,
            Some(ForeignKeyReference {
                model: Identifier::new("testapp__parent"),
                field: Identifier::new("id"),
                on_delete: ForeignKeyOnDeletePolicy::Restrict,
                on_update: ForeignKeyOnUpdatePolicy::Restrict,
            })
        );
    }

    #[test]
    fn test_migration_wrapper() {
        let migration = MigrationWrapper::new(TestMigration);

        assert_eq!(migration.app_name(), "testapp");
        assert_eq!(migration.name(), "m_0001_initial");
        assert_eq!(migration.operations().len(), 1);
    }

    macro_rules! has_spec {
        ($column_def:expr, $spec:pat) => {
            $column_def
                .get_column_spec()
                .iter()
                .any(|spec| matches!(spec, $spec))
        };
    }

    #[test]
    fn test_field_to_column_def() {
        let field = Field::new(Identifier::new("id"), ColumnType::Integer)
            .primary_key()
            .auto()
            .null()
            .unique();

        let mut mapper = MockColumnTypeMapper::new();
        mapper
            .expect_sea_query_column_type_for()
            .return_const(sea_query::ColumnType::Integer);
        let column_def = field.as_column_def(&mapper);

        assert_eq!(column_def.get_column_name(), "id");
        assert_eq!(
            column_def.get_column_type(),
            Some(&sea_query::ColumnType::Integer)
        );
        assert!(has_spec!(column_def, ColumnSpec::PrimaryKey));
        assert!(has_spec!(column_def, ColumnSpec::AutoIncrement));
        assert!(has_spec!(column_def, ColumnSpec::Null));
        assert!(has_spec!(column_def, ColumnSpec::UniqueKey));
    }

    #[test]
    fn test_field_to_column_def_without_options() {
        let field = Field::new(Identifier::new("name"), ColumnType::Text);

        let mut mapper = MockColumnTypeMapper::new();
        mapper
            .expect_sea_query_column_type_for()
            .return_const(sea_query::ColumnType::Text);
        let column_def = field.as_column_def(&mapper);

        assert_eq!(column_def.get_column_name(), "name");
        assert_eq!(
            column_def.get_column_type(),
            Some(&sea_query::ColumnType::Text)
        );
        assert!(!has_spec!(column_def, ColumnSpec::PrimaryKey));
        assert!(!has_spec!(column_def, ColumnSpec::AutoIncrement));
        assert!(!has_spec!(column_def, ColumnSpec::Null));
        assert!(!has_spec!(column_def, ColumnSpec::UniqueKey));
    }
}
