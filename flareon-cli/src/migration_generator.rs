use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Debug, Display};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use cargo_toml::Manifest;
use darling::FromMeta;
use flareon::db::migrations::{DynMigration, MigrationEngine};
use flareon_codegen::model::{Field, Model, ModelArgs, ModelOpts, ModelType};
use log::{debug, info, warn};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, Attribute, Meta, UseTree};

use crate::utils::find_cargo_toml;

pub fn make_migrations(path: &Path, options: MigrationGeneratorOptions) -> anyhow::Result<()> {
    match find_cargo_toml(
        &path
            .canonicalize()
            .with_context(|| "unable to canonicalize Cargo.toml path")?,
    ) {
        Some(cargo_toml_path) => {
            let manifest = Manifest::from_path(&cargo_toml_path)
                .with_context(|| "unable to read Cargo.toml")?;
            let crate_name = manifest
                .package
                .with_context(|| "unable to find package in Cargo.toml")?
                .name;

            MigrationGenerator::new(cargo_toml_path, crate_name, options)
                .generate_and_write_migrations()
                .with_context(|| "unable to generate migrations")?;
        }
        None => {
            bail!("Cargo.toml not found in the specified directory or any parent directory.")
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct MigrationGeneratorOptions {
    pub app_name: Option<String>,
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct MigrationGenerator {
    cargo_toml_path: PathBuf,
    crate_name: String,
    options: MigrationGeneratorOptions,
}

impl MigrationGenerator {
    #[must_use]
    pub fn new(
        cargo_toml_path: PathBuf,
        crate_name: String,
        options: MigrationGeneratorOptions,
    ) -> Self {
        Self {
            cargo_toml_path,
            crate_name,
            options,
        }
    }

    fn generate_and_write_migrations(&mut self) -> anyhow::Result<()> {
        let source_files = self.get_source_files()?;

        if let Some(migration) = self.generate_migrations(source_files)? {
            self.write_migration(migration)?;
        }

        Ok(())
    }

    pub fn generate_migrations(
        &mut self,
        source_files: Vec<SourceFile>,
    ) -> anyhow::Result<Option<MigrationToWrite>> {
        let AppState { models, migrations } = self.process_source_files(source_files)?;
        let migration_processor = MigrationProcessor::new(migrations)?;
        let migration_models = migration_processor.latest_models();
        let (modified_models, operations) = self.generate_operations(&models, &migration_models);

        if operations.is_empty() {
            Ok(None)
        } else {
            let migration_name = migration_processor.next_migration_name()?;
            let dependencies = migration_processor.dependencies();
            let content = self.generate_migration_file_content(
                &migration_name,
                &modified_models,
                dependencies,
                operations,
            );
            Ok(Some(MigrationToWrite::new(migration_name, content)))
        }
    }

    fn get_source_files(&mut self) -> anyhow::Result<Vec<SourceFile>> {
        let src_dir = self
            .cargo_toml_path
            .parent()
            .with_context(|| "unable to find parent dir")?
            .join("src");
        let src_dir = src_dir
            .canonicalize()
            .with_context(|| "unable to canonicalize src dir")?;

        let source_file_paths = Self::find_source_files(&src_dir)?;
        let source_files = source_file_paths
            .into_iter()
            .map(|path| {
                Self::parse_file(&src_dir, path.clone())
                    .with_context(|| format!("unable to parse file: {path:?}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(source_files)
    }

    fn find_source_files(src_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in glob::glob(src_dir.join("**/*.rs").to_str().unwrap())
            .with_context(|| "unable to find Rust source files with glob")?
        {
            let path = entry?;
            paths.push(
                path.strip_prefix(src_dir)
                    .expect("path must be in src dir")
                    .to_path_buf(),
            );
        }

        Ok(paths)
    }

    fn process_source_files(&self, source_files: Vec<SourceFile>) -> anyhow::Result<AppState> {
        let mut app_state = AppState::new();

        for source_file in source_files {
            let path = source_file.path.clone();
            self.process_parsed_file(source_file, &mut app_state)
                .with_context(|| format!("unable to find models in file: {path:?}"))?;
        }

        Ok(app_state)
    }

    fn parse_file(src_dir: &Path, path: PathBuf) -> anyhow::Result<SourceFile> {
        let full_path = src_dir.join(&path);
        debug!("Parsing file: {:?}", &full_path);
        let mut file = File::open(&full_path).with_context(|| "unable to open file")?;

        let mut src = String::new();
        file.read_to_string(&mut src)
            .with_context(|| format!("unable to read file: {full_path:?}"))?;

        SourceFile::parse(path, &src)
    }

    fn process_parsed_file(
        &self,
        SourceFile {
            path,
            content: file,
        }: SourceFile,
        app_state: &mut AppState,
    ) -> anyhow::Result<()> {
        let imports = Self::get_imports(&file, &ModulePath::from_fs_path(&path));
        let import_resolver = SymbolResolver::new(imports);

        let mut migration_models = Vec::new();
        for item in file.items {
            if let syn::Item::Struct(mut item) = item {
                for attr in &item.attrs.clone() {
                    if is_model_attr(attr) {
                        import_resolver.resolve_struct(&mut item);

                        let args = Self::args_from_attr(&path, attr)?;
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

    /// Return the list of top-level `use` statements, structs, and constants as
    /// a list of [`VisibleSymbol`]s from the file.
    fn get_imports(file: &syn::File, module_path: &ModulePath) -> Vec<VisibleSymbol> {
        let mut imports = Vec::new();

        for item in &file.items {
            match item {
                syn::Item::Use(item) => {
                    imports.append(&mut VisibleSymbol::from_item_use(item, module_path));
                }
                syn::Item::Struct(item_struct) => {
                    imports.push(VisibleSymbol::from_item_struct(item_struct, module_path));
                }
                syn::Item::Const(item_const) => {
                    imports.push(VisibleSymbol::from_item_const(item_const, module_path));
                }
                _ => {}
            }
        }

        imports
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

    fn generate_migration_file_content(
        &self,
        migration_name: &str,
        modified_models: &[ModelInSource],
        dependencies: Vec<DynDependency>,
        operations: Vec<DynOperation>,
    ) -> String {
        let operations: Vec<_> = operations
            .into_iter()
            .map(|operation| operation.repr())
            .collect();
        let dependencies: Vec<_> = dependencies
            .into_iter()
            .map(|dependency| dependency.repr())
            .collect();

        let app_name = self.options.app_name.as_ref().unwrap_or(&self.crate_name);
        let migration_def = quote! {
            #[derive(Debug, Copy, Clone)]
            pub(super) struct Migration;

            impl ::flareon::db::migrations::Migration for Migration {
                const APP_NAME: &'static str = #app_name;
                const MIGRATION_NAME: &'static str = #migration_name;
                const DEPENDENCIES: &'static [::flareon::db::migrations::MigrationDependency] = &[
                    #(#dependencies,)*
                ];
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

        Self::generate_migration(migration_def, models_def)
    }

    fn write_migration(&self, migration: MigrationToWrite) -> anyhow::Result<()> {
        let src_path = self
            .options
            .output_dir
            .clone()
            .unwrap_or(self.cargo_toml_path.parent().unwrap().join("src"));
        let migration_path = src_path.join("migrations");
        let migration_file = migration_path.join(format!("{}.rs", migration.name));

        std::fs::create_dir_all(&migration_path).with_context(|| {
            format!(
                "unable to create migrations directory: {}",
                migration_path.display()
            )
        })?;

        let mut file = File::create(&migration_file).with_context(|| {
            format!(
                "unable to create migration file: {}",
                migration_file.display()
            )
        })?;
        file.write_all(migration.content.as_bytes())
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
pub struct SourceFile {
    path: PathBuf,
    content: syn::File,
}

impl SourceFile {
    #[must_use]
    pub fn new(path: PathBuf, content: syn::File) -> Self {
        assert!(
            path.is_relative(),
            "path must be relative to the src directory"
        );
        Self { path, content }
    }

    pub fn parse(path: PathBuf, content: &str) -> anyhow::Result<Self> {
        Ok(Self::new(
            path,
            syn::parse_file(content).with_context(|| "unable to parse file")?,
        ))
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

/// Represents a symbol visible in the current module. This might mean there is
/// a `use` statement for a given type, but also, for instance, the type is
/// defined in the current module.
///
/// For instance, for `use std::collections::HashMap;` the `VisibleSymbol `
/// would be:
/// ```ignore
/// # /*
/// VisibleSymbol {
///     alias: "HashMap",
///     full_path: "std::collections::HashMap",
///     kind: VisibleSymbolKind::Use,
/// }
/// # */
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
struct VisibleSymbol {
    alias: String,
    full_path: String,
    kind: VisibleSymbolKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum VisibleSymbolKind {
    Use,
    Struct,
    Const,
}

impl VisibleSymbol {
    #[must_use]
    fn new(alias: &str, full_path: &str, kind: VisibleSymbolKind) -> Self {
        Self {
            alias: alias.to_string(),
            full_path: full_path.to_string(),
            kind,
        }
    }

    fn full_path_parts(&self) -> impl Iterator<Item = &str> {
        self.full_path.split("::")
    }

    fn new_use(alias: &str, full_path: &str) -> Self {
        Self::new(alias, full_path, VisibleSymbolKind::Use)
    }

    fn from_item_use(item: &syn::ItemUse, module_path: &ModulePath) -> Vec<Self> {
        Self::from_tree(&item.tree, module_path)
    }

    fn from_item_struct(item: &syn::ItemStruct, module_path: &ModulePath) -> Self {
        let ident = item.ident.to_string();
        let full_path = Self::module_path(module_path, &ident);

        Self {
            alias: ident,
            full_path,
            kind: VisibleSymbolKind::Struct,
        }
    }

    fn from_item_const(item: &syn::ItemConst, module_path: &ModulePath) -> Self {
        let ident = item.ident.to_string();
        let full_path = Self::module_path(module_path, &ident);

        Self {
            alias: ident,
            full_path,
            kind: VisibleSymbolKind::Const,
        }
    }

    fn module_path(module_path: &ModulePath, ident: &str) -> String {
        format!("{module_path}::{ident}")
    }

    fn from_tree(tree: &UseTree, current_module: &ModulePath) -> Vec<Self> {
        match tree {
            UseTree::Path(path) => {
                let ident = path.ident.to_string();
                let resolved_path = if ident == "crate" {
                    current_module.crate_name().to_string()
                } else if ident == "self" {
                    current_module.to_string()
                } else if ident == "super" {
                    current_module.parent().to_string()
                } else {
                    ident
                };

                return Self::from_tree(&path.tree, current_module)
                    .into_iter()
                    .map(|import| {
                        Self::new_use(
                            &import.alias,
                            &format!("{}::{}", resolved_path, import.full_path),
                        )
                    })
                    .collect();
            }
            UseTree::Name(name) => {
                let ident = name.ident.to_string();
                return vec![Self::new_use(&ident, &ident)];
            }
            UseTree::Rename(rename) => {
                return vec![Self::new_use(
                    &rename.rename.to_string(),
                    &rename.ident.to_string(),
                )];
            }
            UseTree::Glob(_) => {
                warn!("Glob imports are not supported");
            }
            UseTree::Group(group) => {
                return group
                    .items
                    .iter()
                    .flat_map(|tree| Self::from_tree(tree, current_module))
                    .collect();
            }
        }

        vec![]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModulePath {
    parts: Vec<String>,
}

impl ModulePath {
    #[must_use]
    fn from_fs_path(path: &Path) -> Self {
        let mut parts = vec![String::from("crate")];

        if path == Path::new("lib.rs") || path == Path::new("main.rs") {
            return Self { parts };
        }

        parts.append(
            &mut path
                .components()
                .map(|c| {
                    let component_str = c.as_os_str().to_string_lossy();
                    component_str
                        .strip_suffix(".rs")
                        .unwrap_or(&component_str)
                        .to_string()
                })
                .collect::<Vec<_>>(),
        );

        if parts
            .last()
            .expect("parts must have at least one component")
            == "mod"
        {
            parts.pop();
        }

        Self { parts }
    }

    #[must_use]
    fn parent(&self) -> Self {
        let mut parts = self.parts.clone();
        parts.pop();
        Self { parts }
    }

    #[must_use]
    fn crate_name(&self) -> &str {
        &self.parts[0]
    }
}

impl Display for ModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.parts.join("::"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolResolver {
    /// List of imports in the format `"HashMap" -> VisibleSymbol`
    symbols: HashMap<String, VisibleSymbol>,
}

impl SymbolResolver {
    #[must_use]
    fn new(symbols: Vec<VisibleSymbol>) -> Self {
        let mut symbol_map = HashMap::new();
        for symbol in symbols {
            symbol_map.insert(symbol.alias.clone(), symbol);
        }

        Self {
            symbols: symbol_map,
        }
    }

    fn resolve_struct(&self, item: &mut syn::ItemStruct) {
        for field in &mut item.fields {
            if let syn::Type::Path(path) = &mut field.ty {
                self.resolve(path);
            }
        }
    }

    /// Checks the provided `TypePath` and resolves the full type path, if
    /// available.
    fn resolve(&self, path: &mut syn::TypePath) {
        let first_segment = path.path.segments.first();

        if let Some(first_segment) = first_segment {
            if let Some(symbol) = self.symbols.get(&first_segment.ident.to_string()) {
                let mut new_segments: Vec<_> = symbol
                    .full_path_parts()
                    .map(|s| syn::PathSegment {
                        ident: syn::Ident::new(s, first_segment.ident.span()),
                        arguments: syn::PathArguments::None,
                    })
                    .collect();

                let first_arguments = first_segment.arguments.clone();
                new_segments
                    .last_mut()
                    .expect("new_segments must have at least one element")
                    .arguments = first_arguments;

                new_segments.extend(path.path.segments.iter().skip(1).cloned());
                path.path.segments = syn::punctuated::Punctuated::from_iter(new_segments);
            }

            for segment in &mut path.path.segments {
                self.resolve_path_arguments(&mut segment.arguments);
            }
        }
    }

    fn resolve_path_arguments(&self, arguments: &mut syn::PathArguments) {
        if let syn::PathArguments::AngleBracketed(args) = arguments {
            for arg in &mut args.args {
                self.resolve_generic_argument(arg);
            }
        }
    }

    fn resolve_generic_argument(&self, arg: &mut syn::GenericArgument) {
        if let syn::GenericArgument::Type(syn::Type::Path(path)) = arg {
            if let Some(new_arg) = self.try_resolve_generic_const(path) {
                *arg = new_arg;
            } else {
                self.resolve(path);
            }
        }
    }

    fn try_resolve_generic_const(&self, path: &syn::TypePath) -> Option<syn::GenericArgument> {
        if path.qself.is_none() && path.path.segments.len() == 1 {
            let segment = path
                .path
                .segments
                .first()
                .expect("segments have exactly one element");
            if segment.arguments.is_none() {
                let ident = segment.ident.to_string();
                if let Some(symbol) = self.symbols.get(&ident) {
                    if symbol.kind == VisibleSymbolKind::Const {
                        let path = &symbol.full_path;
                        return Some(syn::GenericArgument::Const(
                            syn::parse_str(path).expect("full_path should be a valid path"),
                        ));
                    }
                }
            }
        }

        None
    }
}

/// Helper struct to process already existing migrations.
#[derive(Debug, Clone)]
struct MigrationProcessor {
    migrations: Vec<Migration>,
}

impl MigrationProcessor {
    fn new(mut migrations: Vec<Migration>) -> anyhow::Result<Self> {
        MigrationEngine::sort_migrations(&mut migrations)?;
        Ok(Self { migrations })
    }

    /// Returns the latest (in the order of applying migrations) versions of the
    /// models that are marked as migration models, that means the latest
    /// version of each migration model.
    ///
    /// This is useful for generating migrations - we can compare the latest
    /// version of the model in the source code with the latest version of the
    /// model in the migrations (returned by this method) and generate the
    /// necessary operations.
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

    fn dependencies(&self) -> Vec<DynDependency> {
        if self.migrations.is_empty() {
            return Vec::new();
        }

        let last_migration = self.migrations.last().unwrap();
        vec![DynDependency::Migration {
            app: last_migration.app_name.clone(),
            migration: last_migration.name.clone(),
        }]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelInSource {
    model_item: syn::ItemStruct,
    model: Model,
}

impl ModelInSource {
    fn from_item(item: syn::ItemStruct, args: &ModelArgs) -> anyhow::Result<Self> {
        let input: syn::DeriveInput = item.clone().into();
        let opts = ModelOpts::new_from_derive_input(&input)
            .map_err(|e| anyhow::anyhow!("cannot parse model: {}", e))?;
        let model = opts.as_model(args)?;

        Ok(Self {
            model_item: item,
            model,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MigrationToWrite {
    pub name: String,
    pub content: String,
}

impl MigrationToWrite {
    #[must_use]
    pub fn new(name: String, content: String) -> Self {
        Self { name, content }
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
            ::flareon::db::migrations::Field::new(::flareon::db::Identifier::new(#column_name), <#ty as ::flareon::db::DatabaseField>::TYPE)
        };
        if self.auto_value {
            tokens = quote! { #tokens.auto() }
        }
        if self.primary_key {
            tokens = quote! { #tokens.primary_key() }
        }
        tokens = quote! { #tokens.set_null(<#ty as ::flareon::db::DatabaseField>::NULLABLE) };
        if self.unique {
            tokens = quote! { #tokens.unique() }
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

    fn dependencies(&self) -> &[flareon::db::migrations::MigrationDependency] {
        &[]
    }

    fn operations(&self) -> &[flareon::db::migrations::Operation] {
        &[]
    }
}

/// A version of [`flareon::db::migrations::MigrationDependency`] that can be
/// created at runtime and is using codegen types.
///
/// This is used to generate migration files.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DynDependency {
    Migration { app: String, migration: String },
    Model { app: String, model_name: String },
}

impl Repr for DynDependency {
    fn repr(&self) -> TokenStream {
        match self {
            Self::Migration { app, migration } => {
                quote! {
                    ::flareon::db::migrations::MigrationDependency::migration(#app, #migration)
                }
            }
            Self::Model { app, model_name } => {
                quote! {
                    ::flareon::db::migrations::MigrationDependency::model(#app, #model_name)
                }
            }
        }
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(source) = &self.source {
            write!(f, "\n{source}")?;
        }
        write!(f, "\n    at {}:{}", self.path.display(), self.location)?;
        Ok(())
    }
}

impl Error for ParsingError {}

#[cfg(test)]
mod tests {
    use quote::ToTokens;

    use super::*;

    #[test]
    fn migration_processor_next_migration_name_empty() {
        let migrations = vec![];
        let processor = MigrationProcessor::new(migrations).unwrap();

        let next_migration_name = processor.next_migration_name().unwrap();
        assert_eq!(next_migration_name, "m_0001_initial");
    }

    #[test]
    fn migration_processor_dependencies_empty() {
        let migrations = vec![];
        let processor = MigrationProcessor::new(migrations).unwrap();

        let next_migration_name = processor.dependencies();
        assert_eq!(next_migration_name, vec![]);
    }

    #[test]
    fn migration_processor_dependencies_previous() {
        let migrations = vec![Migration {
            app_name: "app1".to_string(),
            name: "m0001_initial".to_string(),
            models: vec![],
        }];
        let processor = MigrationProcessor::new(migrations).unwrap();

        let next_migration_name = processor.dependencies();
        assert_eq!(
            next_migration_name,
            vec![DynDependency::Migration {
                app: "app1".to_string(),
                migration: "m0001_initial".to_string(),
            }]
        );
    }

    #[test]
    fn imports() {
        let source = r"
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};
use std::fs::*;
use rand as r;
use super::MyModel;
use crate::MyOtherModel;
use self::MyThirdModel;

struct MyFourthModel {}

const MY_CONSTANT: u8 = 42;
        ";

        let file = SourceFile::parse(PathBuf::from("foo/bar.rs").clone(), source).unwrap();
        let imports =
            MigrationGenerator::get_imports(&file.content, &ModulePath::from_fs_path(&file.path));

        let expected = vec![
            VisibleSymbol {
                alias: "HashMap".to_string(),
                full_path: "std::collections::HashMap".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "StdError".to_string(),
                full_path: "std::error::Error".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "Debug".to_string(),
                full_path: "std::fmt::Debug".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "Display".to_string(),
                full_path: "std::fmt::Display".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "Formatter".to_string(),
                full_path: "std::fmt::Formatter".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "r".to_string(),
                full_path: "rand".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyModel".to_string(),
                full_path: "crate::foo::MyModel".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyOtherModel".to_string(),
                full_path: "crate::MyOtherModel".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyThirdModel".to_string(),
                full_path: "crate::foo::bar::MyThirdModel".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyFourthModel".to_string(),
                full_path: "crate::foo::bar::MyFourthModel".to_string(),
                kind: VisibleSymbolKind::Struct,
            },
            VisibleSymbol {
                alias: "MY_CONSTANT".to_string(),
                full_path: "crate::foo::bar::MY_CONSTANT".to_string(),
                kind: VisibleSymbolKind::Const,
            },
        ];
        assert_eq!(imports, expected);
    }

    #[test]
    fn import_resolver() {
        let resolver = SymbolResolver::new(vec![
            VisibleSymbol::new_use("MyType", "crate::models::MyType"),
            VisibleSymbol::new_use("HashMap", "std::collections::HashMap"),
        ]);

        let path = &mut parse_quote!(MyType);
        resolver.resolve(path);
        assert_eq!(
            quote!(crate::models::MyType).to_string(),
            path.into_token_stream().to_string()
        );

        let path = &mut parse_quote!(HashMap<String, u8>);
        resolver.resolve(path);
        assert_eq!(
            quote!(std::collections::HashMap<String, u8>).to_string(),
            path.into_token_stream().to_string()
        );

        let path = &mut parse_quote!(Option<MyType>);
        resolver.resolve(path);
        assert_eq!(
            quote!(Option<crate::models::MyType>).to_string(),
            path.into_token_stream().to_string()
        );
    }

    #[test]
    fn import_resolver_resolve_struct() {
        let resolver = SymbolResolver::new(vec![
            VisibleSymbol::new_use("MyType", "crate::models::MyType"),
            VisibleSymbol::new_use("HashMap", "std::collections::HashMap"),
            VisibleSymbol::new_use("LimitedString", "flareon::db::LimitedString"),
            VisibleSymbol::new(
                "MY_CONSTANT",
                "crate::constants::MY_CONSTANT",
                VisibleSymbolKind::Const,
            ),
        ]);

        let mut actual = parse_quote! {
            struct Example {
                field_1: MyType,
                field_2: HashMap<String, MyType>,
                field_3: Option<String>,
                field_4: LimitedString<MY_CONSTANT>,
            }
        };
        resolver.resolve_struct(&mut actual);
        let expected = quote! {
            struct Example {
                field_1: crate::models::MyType,
                field_2: std::collections::HashMap<String, crate::models::MyType>,
                field_3: Option<String>,
                field_4: flareon::db::LimitedString<{ crate::constants::MY_CONSTANT }>,
            }
        };
        assert_eq!(actual.into_token_stream().to_string(), expected.to_string());
    }
}
