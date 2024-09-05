use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use cargo_toml::Manifest;
use darling::{FromDeriveInput, FromMeta};
use flareon::db::migrations::{DynMigration, MigrationEngine};
use flareon_codegen::model::{Field, Model, ModelArgs, ModelOpts, ModelType};
use log::{debug, info};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, Attribute, ItemStruct, Meta};

use crate::utils::find_cargo_toml;

pub fn make_migrations(path: &Path) -> anyhow::Result<()> {
    match find_cargo_toml(path) {
        Some(cargo_toml_path) => {
            let manifest = Manifest::from_path(&cargo_toml_path)
                .with_context(|| "unable to read Cargo.toml")?;
            let crate_name = manifest
                .package
                .with_context(|| "unable to find package in Cargo.toml")?
                .name;

            MigrationGenerator::new(cargo_toml_path, crate_name)
                .generate_migrations()
                .with_context(|| "unable to generate migrations")?;
        }
        None => {
            bail!("Cargo.toml not found in the specified directory or any parent directory.")
        }
    }

    Ok(())
}

#[derive(Debug)]
struct MigrationGenerator {
    cargo_toml_path: PathBuf,
    crate_name: String,
}

impl MigrationGenerator {
    #[must_use]
    fn new(cargo_toml_path: PathBuf, crate_name: String) -> Self {
        Self {
            cargo_toml_path,
            crate_name,
        }
    }

    fn generate_migrations(&mut self) -> anyhow::Result<()> {
        let source_file_paths = self.find_source_files()?;
        let AppState { models, migrations } = self.process_source_files(&source_file_paths)?;
        let migration_processor = MigrationProcessor::new(migrations);
        let migration_models = migration_processor.latest_models();
        let (modified_models, operations) = self.generate_operations(&models, &migration_models);
        if !operations.is_empty() {
            self.generate_migration_file(
                &migration_processor.next_migration_name()?,
                &modified_models,
                operations,
            )?;
        }
        Ok(())
    }

    fn find_source_files(&self) -> anyhow::Result<Vec<PathBuf>> {
        let src_dir = self
            .cargo_toml_path
            .parent()
            .with_context(|| "unable to find parent dir")?
            .join("src");
        let src_dir = src_dir
            .canonicalize()
            .with_context(|| "unable to canonicalize src dir")?;

        let mut source_files = Vec::new();
        for entry in glob::glob(src_dir.join("**/*.rs").to_str().unwrap())
            .with_context(|| "unable to find Rust source files with glob")?
        {
            let path = entry?;
            source_files.push(path);
        }

        Ok(source_files)
    }

    fn process_source_files(&self, paths: &Vec<PathBuf>) -> anyhow::Result<AppState> {
        let mut app_state = AppState::new();

        for path in paths {
            self.process_file(path, &mut app_state)
                .with_context(|| format!("unable to find models in file: {path:?}"))?;
        }

        Ok(app_state)
    }

    fn process_file(&self, path: &PathBuf, app_state: &mut AppState) -> anyhow::Result<()> {
        debug!("Parsing file: {:?}", path);
        let mut file = File::open(path).with_context(|| "unable to open file")?;

        let mut src = String::new();
        file.read_to_string(&mut src)
            .with_context(|| format!("unable to read file: {path:?}"))?;

        let syntax = syn::parse_file(&src).with_context(|| "unable to parse file")?;

        let mut migration_models = Vec::new();
        for item in syntax.items {
            if let syn::Item::Struct(item) = item {
                for attr in &item.attrs {
                    if is_model_attr(attr) {
                        let args = Self::args_from_attr(path, attr)?;
                        let model_in_source = ModelInSource::from_item(item, &args)?;

                        match args.model_type {
                            ModelType::Application => app_state.models.push(model_in_source),
                            ModelType::Migration => migration_models.push(model_in_source),
                            ModelType::Internal => {}
                        }

                        break;
                    }
                }
            }
        }

        if !migration_models.is_empty() {
            let migration_name = path
                .file_stem()
                .with_context(|| format!("unable to get migration file name: {}", path.display()))?
                .to_string_lossy()
                .to_string();
            app_state.migrations.push(Migration {
                app_name: self.crate_name.clone(),
                name: migration_name,
                models: migration_models,
            });
        }

        Ok(())
    }

    fn args_from_attr(path: &Path, attr: &Attribute) -> Result<ModelArgs, ParsingError> {
        match attr.meta {
            Meta::Path(_) => {
                // Means `#[model]` without any arguments
                Ok(ModelArgs::default())
            }
            _ => ModelArgs::from_meta(&attr.meta).map_err(|e| {
                ParsingError::from_darling(
                    "couldn't parse model macro arguments",
                    path.to_owned(),
                    &e,
                )
            }),
        }
    }

    #[must_use]
    fn generate_operations(
        &self,
        app_models: &Vec<ModelInSource>,
        migration_models: &Vec<ModelInSource>,
    ) -> (Vec<ModelInSource>, Vec<DynOperation>) {
        let mut operations = Vec::new();
        let mut modified_models = Vec::new();

        let mut all_model_names = HashSet::new();
        let mut app_models_map = HashMap::new();
        for model in app_models {
            all_model_names.insert(model.model.table_name.clone());
            app_models_map.insert(model.model.table_name.clone(), model);
        }
        let mut migration_models_map = HashMap::new();
        for model in migration_models {
            all_model_names.insert(model.model.table_name.clone());
            migration_models_map.insert(model.model.table_name.clone(), model);
        }
        let mut all_model_names: Vec<_> = all_model_names.into_iter().collect();
        all_model_names.sort();

        for model_name in all_model_names {
            let app_model = app_models_map.get(&model_name);
            let migration_model = migration_models_map.get(&model_name);

            match (app_model, migration_model) {
                (Some(&app_model), None) => {
                    operations.push(Self::make_create_model_operation(app_model));
                    modified_models.push(app_model.clone());
                }
                (Some(&app_model), Some(&migration_model)) => {
                    if app_model.model != migration_model.model {
                        modified_models.push(app_model.clone());
                        operations
                            .extend(self.make_alter_model_operations(app_model, migration_model));
                    }
                }
                (None, Some(&migration_model)) => {
                    operations.push(self.make_remove_model_operation(migration_model));
                }
                (None, None) => unreachable!(),
            }
        }

        (modified_models, operations)
    }

    #[must_use]
    fn make_create_model_operation(app_model: &ModelInSource) -> DynOperation {
        DynOperation::CreateModel {
            table_name: app_model.model.table_name.clone(),
            fields: app_model.model.fields.clone(),
        }
    }

    #[must_use]
    fn make_alter_model_operations(
        &self,
        app_model: &ModelInSource,
        migration_model: &ModelInSource,
    ) -> Vec<DynOperation> {
        let mut all_field_names = HashSet::new();
        let mut app_model_fields = HashMap::new();
        for field in &app_model.model.fields {
            all_field_names.insert(field.column_name.clone());
            app_model_fields.insert(field.column_name.clone(), field);
        }
        let mut migration_model_fields = HashMap::new();
        for field in &migration_model.model.fields {
            all_field_names.insert(field.column_name.clone());
            migration_model_fields.insert(field.column_name.clone(), field);
        }

        let mut all_field_names: Vec<_> = all_field_names.into_iter().collect();
        all_field_names.sort();

        let mut operations = Vec::new();
        for field_name in all_field_names {
            let app_field = app_model_fields.get(&field_name);
            let migration_field = migration_model_fields.get(&field_name);

            match (app_field, migration_field) {
                (Some(app_field), None) => {
                    operations.push(Self::make_add_field_operation(app_model, app_field));
                }
                (Some(app_field), Some(migration_field)) => {
                    let operation = self.make_alter_field_operation(
                        app_model,
                        app_field,
                        migration_model,
                        migration_field,
                    );
                    if let Some(operation) = operation {
                        operations.push(operation);
                    }
                }
                (None, Some(migration_field)) => {
                    operations
                        .push(self.make_remove_field_operation(migration_model, migration_field));
                }
                (None, None) => unreachable!(),
            }
        }

        operations
    }

    #[must_use]
    fn make_add_field_operation(app_model: &ModelInSource, field: &Field) -> DynOperation {
        DynOperation::AddField {
            table_name: app_model.model.table_name.clone(),
            field: field.clone(),
        }
    }

    #[must_use]
    fn make_alter_field_operation(
        &self,
        _app_model: &ModelInSource,
        app_field: &Field,
        _migration_model: &ModelInSource,
        migration_field: &Field,
    ) -> Option<DynOperation> {
        if app_field == migration_field {
            return None;
        }
        todo!()
    }

    #[must_use]
    fn make_remove_field_operation(
        &self,
        _migration_model: &ModelInSource,
        _migration_field: &Field,
    ) -> DynOperation {
        todo!()
    }

    #[must_use]
    fn make_remove_model_operation(&self, _migration_model: &ModelInSource) -> DynOperation {
        todo!()
    }

    fn generate_migration_file(
        &self,
        migration_name: &str,
        modified_models: &[ModelInSource],
        operations: Vec<DynOperation>,
    ) -> anyhow::Result<()> {
        let operations: Vec<_> = operations
            .into_iter()
            .map(|operation| operation.repr())
            .collect();

        let app_name = &self.crate_name;
        let migration_def = quote! {
            pub(super) struct Migration;

            impl ::flareon::db::migrations::Migration for Migration {
                const APP_NAME: &'static str = #app_name;
                const MIGRATION_NAME: &'static str = #migration_name;
                const OPERATIONS: &'static [::flareon::db::migrations::Operation] = &[
                    #(#operations,)*
                ];
            }
        };

        let models = modified_models
            .iter()
            .map(Self::model_to_migration_model)
            .collect::<Vec<_>>();
        let models_def = quote! {
            #(#models)*
        };

        let migration_path = self
            .cargo_toml_path
            .parent()
            .unwrap()
            .join("src")
            .join("migrations");
        let migration_file = migration_path.join(format!("{migration_name}.rs"));
        let migration_content = Self::generate_migration(migration_def, models_def);

        let mut file = File::create(&migration_file).with_context(|| {
            format!(
                "unable to create migration file: {}",
                migration_file.display()
            )
        })?;
        file.write_all(migration_content.as_bytes())
            .with_context(|| "unable to write migration file")?;
        info!("Generated migration: {}", migration_file.display());

        Ok(())
    }

    #[must_use]
    fn generate_migration(migration: TokenStream, modified_models: TokenStream) -> String {
        let migration = Self::format_tokens(migration);
        let modified_models = Self::format_tokens(modified_models);

        let version = env!("CARGO_PKG_VERSION");
        let date_time = chrono::offset::Utc::now().format("%Y-%m-%d %H:%M:%S%:z");
        let header = format!("//! Generated by flareon CLI {version} on {date_time}");

        format!("{header}\n\n{migration}\n{modified_models}")
    }

    fn format_tokens(tokens: TokenStream) -> String {
        let parsed: syn::File = syn::parse2(tokens).unwrap();
        prettyplease::unparse(&parsed)
    }

    #[must_use]
    fn model_to_migration_model(model: &ModelInSource) -> proc_macro2::TokenStream {
        let mut model_source = model.model_item.clone();
        model_source.vis = syn::Visibility::Inherited;
        model_source.ident = format_ident!("_{}", model_source.ident);
        model_source.attrs.clear();
        model_source
            .attrs
            .push(syn::parse_quote! {#[derive(::core::fmt::Debug)]});
        model_source
            .attrs
            .push(syn::parse_quote! {#[::flareon::db::model(model_type = "migration")]});
        quote! {
            #model_source
        }
    }
}

#[derive(Debug, Clone)]
struct AppState {
    /// All the application models found in the source
    models: Vec<ModelInSource>,
    /// All the migrations found in the source
    migrations: Vec<Migration>,
}

impl AppState {
    #[must_use]
    fn new() -> Self {
        Self {
            models: Vec::new(),
            migrations: Vec::new(),
        }
    }
}

/// Helper struct to process already existing migrations.
#[derive(Debug, Clone)]
struct MigrationProcessor {
    migrations: Vec<Migration>,
}

impl MigrationProcessor {
    #[must_use]
    fn new(mut migrations: Vec<Migration>) -> Self {
        MigrationEngine::sort_migrations(&mut migrations);
        Self { migrations }
    }

    /// Returns the latest (in the order of applying migrations) versions of the
    /// models that are marked as migration models.
    #[must_use]
    fn latest_models(&self) -> Vec<ModelInSource> {
        let mut migration_models: HashMap<String, &ModelInSource> = HashMap::new();
        for migration in &self.migrations {
            for model in &migration.models {
                migration_models.insert(model.model.table_name.clone(), model);
            }
        }

        migration_models.into_values().cloned().collect()
    }

    fn next_migration_name(&self) -> anyhow::Result<String> {
        if self.migrations.is_empty() {
            return Ok("m_0001_initial".to_string());
        }

        let last_migration = self.migrations.last().unwrap();
        let last_migration_number = last_migration
            .name
            .split('_')
            .nth(1)
            .with_context(|| format!("migration number not found: {}", last_migration.name))?
            .parse::<u32>()
            .with_context(|| {
                format!("unable to parse migration number: {}", last_migration.name)
            })?;

        let migration_number = last_migration_number + 1;
        let now = chrono::Utc::now();
        let date_time = now.format("%Y%m%d_%H%M%S");

        Ok(format!("m_{migration_number:04}_auto_{date_time}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelInSource {
    model_item: ItemStruct,
    model: Model,
}

impl ModelInSource {
    fn from_item(item: ItemStruct, args: &ModelArgs) -> anyhow::Result<Self> {
        let input: syn::DeriveInput = item.clone().into();
        let opts = ModelOpts::from_derive_input(&input)
            .map_err(|e| anyhow::anyhow!("cannot parse model: {}", e))?;
        let model = opts.as_model(args)?;

        Ok(Self {
            model_item: item,
            model,
        })
    }
}

#[must_use]
fn is_model_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();

    let model_path: syn::Path = parse_quote!(flareon::db::model);
    let model_path_prefixed: syn::Path = parse_quote!(::flareon::db::model);

    attr.style == syn::AttrStyle::Outer
        && (path.is_ident("model") || path == &model_path || path == &model_path_prefixed)
}

trait Repr {
    fn repr(&self) -> proc_macro2::TokenStream;
}

impl Repr for Field {
    fn repr(&self) -> proc_macro2::TokenStream {
        let column_name = &self.column_name;
        let ty = &self.ty;
        let mut tokens = quote! {
            ::flareon::db::migrations::Field::new(::flareon::db::Identifier::new(#column_name), <#ty as ::flareon::db::DbField>::TYPE)
        };
        if self.auto_value {
            tokens = quote! { #tokens.auto() }
        }
        if self.primary_key {
            tokens = quote! { #tokens.primary_key() }
        }
        tokens
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Migration {
    app_name: String,
    name: String,
    models: Vec<ModelInSource>,
}

impl DynMigration for Migration {
    fn app_name(&self) -> &str {
        &self.app_name
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn operations(&self) -> &[flareon::db::migrations::Operation] {
        &[]
    }
}

/// A version of [`flareon::db::migrations::Operation`] that can be created at
/// runtime and is using codegen types.
///
/// This is used to generate migration files.
#[derive(Debug, Clone)]
enum DynOperation {
    CreateModel {
        table_name: String,
        fields: Vec<Field>,
    },
    AddField {
        table_name: String,
        field: Field,
    },
}

impl Repr for DynOperation {
    fn repr(&self) -> TokenStream {
        match self {
            Self::CreateModel { table_name, fields } => {
                let fields = fields.iter().map(Repr::repr).collect::<Vec<_>>();
                quote! {
                    ::flareon::db::migrations::Operation::create_model()
                        .table_name(::flareon::db::Identifier::new(#table_name))
                        .fields(&[
                            #(#fields,)*
                        ])
                        .build()
                }
            }
            Self::AddField { table_name, field } => {
                let field = field.repr();
                quote! {
                    ::flareon::db::migrations::Operation::add_field()
                        .table_name(::flareon::db::Identifier::new(#table_name))
                        .field(#field)
                        .build()
                }
            }
        }
    }
}

#[derive(Debug)]
struct ParsingError {
    message: String,
    path: PathBuf,
    location: String,
    source: Option<String>,
}

impl ParsingError {
    fn from_darling(message: &str, path: PathBuf, error: &darling::Error) -> Self {
        let message = format!("{message}: {error}");
        let span = error.span();
        let location = format!("{}:{}", span.start().line, span.start().column);

        Self {
            message,
            path,
            location,
            source: span.source_text().clone(),
        }
    }
}

impl Display for ParsingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(source) = &self.source {
            write!(f, "\n{source}")?;
        }
        write!(f, "\n    at {}:{}", self.path.display(), self.location)?;
        Ok(())
    }
}

impl Error for ParsingError {}
