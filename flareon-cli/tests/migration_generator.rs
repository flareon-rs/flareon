use std::path::PathBuf;

use flareon_cli::migration_generator::{
    DynOperation, MigrationAsSource, MigrationGenerator, MigrationGeneratorOptions, SourceFile,
};

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
    if let DynOperation::CreateModel { table_name, fields } = &migration.operations[0] {
        assert_eq!(table_name, "my_model");
        assert_eq!(fields.len(), 3);

        let field = &fields[0];
        assert_eq!(field.column_name, "id");
        assert!(field.primary_key);
        assert!(field.auto_value.unwrap());
        assert!(!field.foreign_key.unwrap());

        let field = &fields[1];
        assert_eq!(field.column_name, "field_1");
        assert!(!field.primary_key);
        assert!(!field.auto_value.unwrap());
        assert!(!field.foreign_key.unwrap());

        let field = &fields[2];
        assert_eq!(field.column_name, "field_2");
        assert!(!field.primary_key);
        assert!(!field.auto_value.unwrap());
        assert!(!field.foreign_key.unwrap());
    }
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
