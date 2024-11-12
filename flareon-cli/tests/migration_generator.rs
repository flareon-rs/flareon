use std::path::PathBuf;

use flareon_cli::migration_generator::{
    MigrationGenerator, MigrationGeneratorOptions, MigrationToWrite, SourceFile,
};

/// Test that the migration generator can generate a create model migration for
/// a given model which compiles successfully.
#[test]
fn create_model_compile_test() {
    let mut generator = MigrationGenerator::new(
        PathBuf::from("Cargo.toml"),
        String::from("my_crate"),
        MigrationGeneratorOptions::default(),
    );
    let src = include_str!("migration_generator/create_model.rs");
    let source_files = vec![SourceFile::parse(PathBuf::from("main.rs"), src).unwrap()];

    let migration_opt = generator.generate_migrations(source_files).unwrap();
    let MigrationToWrite {
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
