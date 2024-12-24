use convert_case::{Case, Casing};
use darling::{FromDeriveInput, FromField, FromMeta};

use crate::symbol_resolver::SymbolResolver;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Default, FromMeta)]
pub struct ModelArgs {
    #[darling(default)]
    pub model_type: ModelType,
    pub table_name: Option<String>,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default, FromMeta)]
pub enum ModelType {
    #[default]
    Application,
    Migration,
    Internal,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, FromDeriveInput)]
#[darling(forward_attrs(allow, doc, cfg), supports(struct_named))]
pub struct ModelOpts {
    pub ident: syn::Ident,
    pub generics: syn::Generics,
    pub data: darling::ast::Data<darling::util::Ignored, FieldOpts>,
}

impl ModelOpts {
    pub fn new_from_derive_input(input: &syn::DeriveInput) -> Result<Self, darling::error::Error> {
        let opts = Self::from_derive_input(input)?;
        if !opts.generics.params.is_empty() {
            return Err(
                darling::Error::custom("generics in models are not supported")
                    .with_span(&opts.generics),
            );
        }
        Ok(opts)
    }

    /// Get the fields of the struct.
    ///
    /// # Panics
    ///
    /// Panics if the [`ModelOpts`] was not parsed from a struct.
    #[must_use]
    pub fn fields(&self) -> Vec<&FieldOpts> {
        self.data
            .as_ref()
            .take_struct()
            .expect("Only structs are supported")
            .fields
    }

    /// Convert the model options into a model.
    ///
    /// # Errors
    ///
    /// Returns an error if the model name does not start with an underscore
    /// when the model type is [`ModelType::Migration`].
    pub fn as_model(
        &self,
        args: &ModelArgs,
        symbol_resolver: Option<&SymbolResolver>,
    ) -> Result<Model, syn::Error> {
        let fields: Vec<_> = self
            .fields()
            .iter()
            .map(|field| field.as_field(symbol_resolver))
            .collect();

        let mut original_name = self.ident.to_string();
        if args.model_type == ModelType::Migration {
            original_name = original_name
                .strip_prefix("_")
                .ok_or_else(|| {
                    syn::Error::new(
                        self.ident.span(),
                        "migration model names must start with an underscore",
                    )
                })?
                .to_string();
        }
        let table_name = if let Some(table_name) = &args.table_name {
            table_name.clone()
        } else {
            original_name.to_string().to_case(Case::Snake)
        };

        let primary_key_field = self.get_primary_key_field(&fields)?;

        Ok(Model {
            name: self.ident.clone(),
            original_name,
            model_type: args.model_type,
            table_name,
            pk_field: primary_key_field.clone(),
            fields,
        })
    }

    fn get_primary_key_field<'a>(&self, fields: &'a [Field]) -> Result<&'a Field, syn::Error> {
        let pks: Vec<_> = fields.iter().filter(|field| field.primary_key).collect();
        if pks.is_empty() {
            return Err(syn::Error::new(
                self.ident.span(),
                "models must have a primary key field, either named `id` \
                or annotated with the `#[model(primary_key)]` attribute",
            ));
        }
        if pks.len() > 1 {
            return Err(syn::Error::new(
                pks[1].field_name.span(),
                "composite primary keys are not supported; only one primary key field is allowed",
            ));
        }

        Ok(pks[0])
    }
}

#[derive(Debug, Clone, FromField)]
#[darling(attributes(model))]
pub struct FieldOpts {
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
    pub primary_key: darling::util::Flag,
    pub unique: darling::util::Flag,
}

impl FieldOpts {
    #[must_use]
    fn has_type(&self, type_to_check: &str, symbol_resolver: &SymbolResolver) -> bool {
        let mut ty = self.ty.clone();
        symbol_resolver.resolve(&mut ty);
        Self::inner_type_names(&ty)
            .iter()
            .any(|name| name == type_to_check)
    }

    #[must_use]
    fn inner_type_names(ty: &syn::Type) -> Vec<String> {
        let mut names = Vec::new();
        Self::inner_type_names_impl(ty, &mut names);
        names
    }

    fn inner_type_names_impl(ty: &syn::Type, names: &mut Vec<String>) {
        if let syn::Type::Path(type_path) = ty {
            let name = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            names.push(name);

            for arg in &type_path.path.segments {
                if let syn::PathArguments::AngleBracketed(arg) = &arg.arguments {
                    for arg in &arg.args {
                        if let syn::GenericArgument::Type(ty) = arg {
                            Self::inner_type_names_impl(ty, names);
                        }
                    }
                }
            }
        }
    }

    fn find_type(&self, type_to_find: &str, symbol_resolver: &SymbolResolver) -> Option<syn::Type> {
        let mut ty = self.ty.clone();
        symbol_resolver.resolve(&mut ty);
        Self::find_type_resolved(&ty, type_to_find)
    }

    fn find_type_resolved(ty: &syn::Type, type_to_find: &str) -> Option<syn::Type> {
        if let syn::Type::Path(type_path) = ty {
            let name = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            if name == type_to_find {
                return Some(ty.clone());
            }

            for arg in &type_path.path.segments {
                if let syn::PathArguments::AngleBracketed(arg) = &arg.arguments {
                    if let Some(ty) = Self::find_type_in_generics(arg, type_to_find) {
                        return Some(ty);
                    }
                }
            }
        }

        None
    }

    fn find_type_in_generics(
        arg: &syn::AngleBracketedGenericArguments,
        type_to_find: &str,
    ) -> Option<syn::Type> {
        arg.args
            .iter()
            .filter_map(|arg| {
                if let syn::GenericArgument::Type(ty) = arg {
                    Self::find_type_resolved(ty, type_to_find)
                } else {
                    None
                }
            })
            .next()
    }

    /// Convert the field options into a field.
    ///
    /// # Panics
    ///
    /// Panics if the field does not have an identifier (i.e. it is a tuple
    /// struct).
    #[must_use]
    pub fn as_field(&self, symbol_resolver: Option<&SymbolResolver>) -> Field {
        let name = self.ident.as_ref().unwrap();
        let column_name = name.to_string();
        let (auto_value, foreign_key) = match symbol_resolver {
            Some(resolver) => (
                MaybeUnknown::Determined(self.find_type("flareon::db::Auto", resolver).is_some()),
                MaybeUnknown::Determined(
                    self.find_type("flareon::db::ForeignKey", resolver)
                        .map(ForeignKeySpec::from),
                ),
            ),
            None => (MaybeUnknown::Unknown, MaybeUnknown::Unknown),
        };
        let is_primary_key = column_name == "id" || self.primary_key.is_present();

        Field {
            field_name: name.clone(),
            column_name,
            ty: self.ty.clone(),
            auto_value,
            primary_key: is_primary_key,
            foreign_key,
            unique: self.unique.is_present(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Model {
    pub name: syn::Ident,
    pub original_name: String,
    pub model_type: ModelType,
    pub table_name: String,
    pub pk_field: Field,
    pub fields: Vec<Field>,
}

impl Model {
    #[must_use]
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    pub field_name: syn::Ident,
    pub column_name: String,
    pub ty: syn::Type,
    /// Whether the field is an auto field (e.g. `id`);
    /// [`MaybeUnknown::Unknown`] if this `Field` instance was not resolved with
    /// a [`SymbolResolver`].
    pub auto_value: MaybeUnknown<bool>,
    pub primary_key: bool,
    /// [`Some`] wrapped in [`MaybeUnknown::Determined`] if this field is a
    /// foreign key; [`None`] wrapped in [`MaybeUnknown::Determined`] if this
    /// field is determined not to be a foreign key; [`MaybeUnknown::Unknown`]
    /// if this `Field` instance was not resolved with a [`SymbolResolver`].
    pub foreign_key: MaybeUnknown<Option<ForeignKeySpec>>,
    pub unique: bool,
}

/// Wraps a type whose value may or may not be possible to be determined using
/// the information available.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum MaybeUnknown<T> {
    /// Indicates that this instance is determined to be a certain value
    /// (possibly [`None`] if wrapping an [`Option`]).
    Determined(T),
    /// Indicates that the value is unknown.
    Unknown,
}

impl<T> MaybeUnknown<T> {
    pub fn unwrap(self) -> T {
        match self {
            MaybeUnknown::Determined(value) => value,
            MaybeUnknown::Unknown => {
                panic!("called `MaybeUnknown::unwrap()` on an `Unknown` value")
            }
        }
    }

    pub fn expect(self, msg: &str) -> T {
        match self {
            MaybeUnknown::Determined(value) => value,
            MaybeUnknown::Unknown => {
                panic!("{}", msg)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForeignKeySpec {
    pub ty: syn::Type,
}

impl From<syn::Type> for ForeignKeySpec {
    fn from(value: syn::Type) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;
    use syn::TraitBoundModifier::Maybe;

    use super::*;
    #[cfg(feature = "symbol-resolver")]
    use crate::symbol_resolver::{VisibleSymbol, VisibleSymbolKind};

    #[test]
    fn model_args_default() {
        let args: ModelArgs = Default::default();
        assert_eq!(args.model_type, ModelType::Application);
        assert!(args.table_name.is_none());
    }

    #[test]
    fn model_type_default() {
        let model_type: ModelType = Default::default();
        assert_eq!(model_type, ModelType::Application);
    }

    #[test]
    fn model_opts_fields() {
        let input: syn::DeriveInput = parse_quote! {
            struct TestModel {
                id: i32,
                name: String,
            }
        };
        let opts = ModelOpts::new_from_derive_input(&input).unwrap();
        let fields = opts.fields();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].ident.as_ref().unwrap().to_string(), "id");
        assert_eq!(fields[1].ident.as_ref().unwrap().to_string(), "name");
    }

    #[test]
    fn model_opts_as_model() {
        let input: syn::DeriveInput = parse_quote! {
            struct TestModel {
                id: i32,
                name: String,
            }
        };
        let opts = ModelOpts::new_from_derive_input(&input).unwrap();
        let args = ModelArgs::default();
        let model = opts.as_model(&args, None).unwrap();
        assert_eq!(model.name.to_string(), "TestModel");
        assert_eq!(model.table_name, "test_model");
        assert_eq!(model.fields.len(), 2);
        assert_eq!(model.field_count(), 2);
    }

    #[test]
    fn model_opts_as_model_migration() {
        let input: syn::DeriveInput = parse_quote! {
            #[model(model_type = "migration")]
            struct TestModel {
                id: i32,
                name: String,
            }
        };
        let opts = ModelOpts::new_from_derive_input(&input).unwrap();
        let args = ModelArgs::from_meta(&input.attrs.first().unwrap().meta).unwrap();
        let err = opts.as_model(&args, None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "migration model names must start with an underscore"
        );
    }

    #[test]
    fn model_opts_as_model_pk_attr() {
        let input: syn::DeriveInput = parse_quote! {
            #[model]
            struct TestModel {
                #[model(primary_key)]
                name: i32,
            }
        };
        let opts = ModelOpts::new_from_derive_input(&input).unwrap();
        let args = ModelArgs::default();
        let model = opts.as_model(&args, None).unwrap();
        assert_eq!(model.fields.len(), 1);
        assert!(model.fields[0].primary_key);
    }

    #[test]
    fn model_opts_as_model_no_pk() {
        let input: syn::DeriveInput = parse_quote! {
            #[model]
            struct TestModel {
                name: String,
            }
        };
        let opts = ModelOpts::new_from_derive_input(&input).unwrap();
        let args = ModelArgs::default();
        let err = opts.as_model(&args, None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "models must have a primary key field, either named `id` \
            or annotated with the `#[model(primary_key)]` attribute"
        );
    }

    #[test]
    fn model_opts_as_model_multiple_pks() {
        let input: syn::DeriveInput = parse_quote! {
            #[model]
            struct TestModel {
                id: i64,
                #[model(primary_key)]
                id_2: i64,
                name: String,
            }
        };
        let opts = ModelOpts::new_from_derive_input(&input).unwrap();
        let args = ModelArgs::default();
        let err = opts.as_model(&args, None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "composite primary keys are not supported; only one primary key field is allowed"
        );
    }

    #[test]
    fn field_opts_as_field() {
        let input: syn::Field = parse_quote! {
            #[model(unique)]
            name: String
        };
        let field_opts = FieldOpts::from_field(&input).unwrap();
        let field = field_opts.as_field(None);
        assert_eq!(field.field_name.to_string(), "name");
        assert_eq!(field.column_name, "name");
        assert_eq!(field.ty, parse_quote!(String));
        assert!(field.unique);
        assert_eq!(field.auto_value, MaybeUnknown::Unknown);
        assert_eq!(field.foreign_key, MaybeUnknown::Unknown);
    }

    #[test]
    fn inner_type_names() {
        let input: syn::Type =
            parse_quote! { ::my_crate::MyContainer<'a, Vec<std::string::String>> };
        let names = FieldOpts::inner_type_names(&input);
        assert_eq!(
            names,
            vec!["my_crate::MyContainer", "Vec", "std::string::String"]
        );
    }

    #[cfg(feature = "symbol-resolver")]
    #[test]
    fn contains_type() {
        let symbols = vec![VisibleSymbol::new(
            "MyContainer",
            "my_crate::MyContainer",
            VisibleSymbolKind::Use,
        )];
        let resolver = SymbolResolver::new(symbols);

        let opts = FieldOpts {
            ident: None,
            ty: parse_quote! { MyContainer<std::string::String> },
            primary_key: Default::default(),
            unique: Default::default(),
        };

        assert!(opts.has_type("my_crate::MyContainer", &resolver));
        assert!(opts.has_type("std::string::String", &resolver));
        assert!(!opts.has_type("MyContainer", &resolver));
        assert!(!opts.has_type("String", &resolver));
    }
}
