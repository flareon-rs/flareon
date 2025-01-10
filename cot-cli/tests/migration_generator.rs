use std::path::PathBuf;

use cot_cli::migration_generator::{
    DynDependency, DynOperation, MigrationAsSource, MigrationGenerator, MigrationGeneratorOptions,
    SourceFile,
};
use syn::parse_quote;

/// Test that the migration generator can generate a "create model" migration
/// for a given model that has an expected state.
#[test]
fn create_model_state_test() {
    let mut generator = test_generator();
    let src = include_str!("migration_generator/create_model.rs");
    let source_files = vec![SourceFile::parse(PathBuf::from("main.rs"), src).unwrap()];

    let migration = generator
        .generate_migrations(source_files)
        .unwrap()
        .unwrap();

    assert_eq!(migration.migration_name, "m_0001_initial");
    assert!(migration.dependencies.is_empty());

    let (table_name, fields) = unwrap_create_model(&migration.operations[0]);
    assert_eq!(table_name, "parent");
    assert_eq!(fields.len(), 1);

    let (table_name, fields) = unwrap_create_model(&migration.operations[1]);
    assert_eq!(table_name, "my_model");
    assert_eq!(fields.len(), 4);

    let field = &fields[0];
    assert_eq!(field.column_name, "id");
    assert!(field.primary_key);
    assert!(field.auto_value);
    assert!(field.foreign_key.clone().is_none());

    let field = &fields[1];
    assert_eq!(field.column_name, "field_1");
    assert!(!field.primary_key);
    assert!(!field.auto_value);
    assert!(field.foreign_key.clone().is_none());

    let field = &fields[2];
    assert_eq!(field.column_name, "field_2");
    assert!(!field.primary_key);
    assert!(!field.auto_value);
    assert!(field.foreign_key.clone().is_none());

    let field = &fields[3];
    assert_eq!(field.column_name, "parent");
    assert!(!field.primary_key);
    assert!(!field.auto_value);
    assert!(field.foreign_key.clone().is_some());
}

#[test]
fn create_models_foreign_key() {
    let mut generator = test_generator();
    let src = include_str!("migration_generator/foreign_key.rs");
    let source_files = vec![SourceFile::parse(PathBuf::from("main.rs"), src).unwrap()];

    let migration = generator
        .generate_migrations(source_files)
        .unwrap()
        .unwrap();

    assert_eq!(migration.dependencies.len(), 0);
    assert_eq!(migration.operations.len(), 2);

    // Parent must be created before Child
    let (table_name, fields) = unwrap_create_model(&migration.operations[0]);
    assert_eq!(table_name, "parent");
    assert_eq!(fields.len(), 1);

    let (table_name, fields) = unwrap_create_model(&migration.operations[1]);
    assert_eq!(table_name, "child");
    assert_eq!(fields.len(), 2);

    let field = &fields[0];
    assert_eq!(field.column_name, "id");
    assert!(field.primary_key);
    assert!(field.auto_value);
    assert!(field.foreign_key.clone().is_none());

    let field = &fields[1];
    assert_eq!(field.column_name, "parent");
    assert!(!field.primary_key);
    assert!(!field.auto_value);
    assert!(field.foreign_key.clone().is_some());
}

#[test]
fn create_models_foreign_key_cycle() {
    let mut generator = test_generator();
    let src = include_str!("migration_generator/foreign_key_cycle.rs");
    let source_files = vec![SourceFile::parse(PathBuf::from("main.rs"), src).unwrap()];

    let migration = generator
        .generate_migrations(source_files)
        .unwrap()
        .unwrap();

    assert_eq!(migration.dependencies.len(), 0);
    assert_eq!(migration.operations.len(), 3);

    // Parent must be created before Child
    let (table_name, fields) = unwrap_create_model(&migration.operations[0]);
    assert_eq!(table_name, "parent");
    assert_eq!(fields.len(), 1);

    let (table_name, fields) = unwrap_create_model(&migration.operations[1]);
    assert_eq!(table_name, "child");
    assert_eq!(fields.len(), 2);

    let (table_name, field) = unwrap_add_field(&migration.operations[2]);
    assert_eq!(table_name, "parent");
    assert_eq!(field.field_name, "child");
}

#[test]
fn create_models_foreign_key_two_migrations() {
    let mut generator = test_generator();

    let src = include_str!("migration_generator/foreign_key_two_migrations/step_1.rs");
    let source_files = vec![SourceFile::parse(PathBuf::from("main.rs"), src).unwrap()];
    let migration_file = generator
        .generate_migrations_to_write(source_files)
        .unwrap()
        .unwrap();

    let src = include_str!("migration_generator/foreign_key_two_migrations/step_2.rs");
    let source_files = vec![
        SourceFile::parse(PathBuf::from("main.rs"), src).unwrap(),
        SourceFile::parse(PathBuf::from(&migration_file.name), &migration_file.content).unwrap(),
    ];
    let migration = generator
        .generate_migrations(source_files)
        .unwrap()
        .unwrap();

    assert_eq!(migration.dependencies.len(), 2);
    assert!(migration.dependencies.contains(&DynDependency::Migration {
        app: "my_crate".to_string(),
        migration: "m_0001_initial".to_string()
    }));
    assert!(migration.dependencies.contains(&DynDependency::Model {
        model_type: parse_quote!(crate::Parent),
    }));

    assert_eq!(migration.operations.len(), 1);

    let (table_name, _fields) = unwrap_create_model(&migration.operations[0]);
    assert_eq!(table_name, "child");
}

/// Test that the migration generator can generate a "create model" migration
/// for a given model which compiles successfully.
#[test]
#[cfg_attr(miri, ignore)] // unsupported operation: extern static `pidfd_spawnp` is not supported by Miri
fn create_model_compile_test() {
    let mut generator = test_generator();
    let src = include_str!("migration_generator/create_model.rs");
    let source_files = vec![SourceFile::parse(PathBuf::from("main.rs"), src).unwrap()];

    let migration_opt = generator
        .generate_migrations_to_write(source_files)
        .unwrap();
    let MigrationAsSource {
        name: migration_name,
        content: migration_content,
    } = migration_opt.unwrap();

    let source_with_migrations = format!(
        r"
{src}

mod migrations {{
    mod {migration_name} {{
        {migration_content}
    }}
}}"
    );

    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("main.rs");
    std::fs::write(&test_path, source_with_migrations).unwrap();

    let t = trybuild::TestCases::new();
    t.pass(&test_path);
}

fn test_generator() -> MigrationGenerator {
    MigrationGenerator::new(
        PathBuf::from("Cargo.toml"),
        String::from("my_crate"),
        MigrationGeneratorOptions::default(),
    )
}

fn unwrap_create_model(op: &DynOperation) -> (&str, Vec<cot_codegen::model::Field>) {
    if let DynOperation::CreateModel {
        table_name, fields, ..
    } = op
    {
        (table_name, fields.clone())
    } else {
        panic!("expected create model operation");
    }
}

fn unwrap_add_field(op: &DynOperation) -> (&str, cot_codegen::model::Field) {
    if let DynOperation::AddField {
        table_name, field, ..
    } = op
    {
        (table_name, field.clone())
    } else {
        panic!("expected create model operation");
    }
}
