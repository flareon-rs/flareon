use convert_case::{Case, Casing};
use darling::{FromDeriveInput, FromField, FromMeta};

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
    pub fn as_model(&self, args: &ModelArgs) -> Result<Model, syn::Error> {
        let fields: Vec<_> = self.fields().iter().map(|field| field.as_field()).collect();

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
    /// Convert the field options into a field.
    ///
    /// # Panics
    ///
    /// Panics if the field does not have an identifier (i.e. it is a tuple
    /// struct).
    #[must_use]
    pub fn as_field(&self) -> Field {
        let name = self.ident.as_ref().unwrap();
        let column_name = name.to_string();
        // TODO define a separate type for auto fields
        let is_auto = column_name == "id";
        let is_primary_key = column_name == "id" || self.primary_key.is_present();

        Field {
            field_name: name.clone(),
            column_name,
            ty: self.ty.clone(),
            auto_value: is_auto,
            primary_key: is_primary_key,
            null: false,
            unique: self.unique.is_present(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub auto_value: bool,
    pub primary_key: bool,
    pub null: bool,
    pub unique: bool,
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

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
        let model = opts.as_model(&args).unwrap();
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
        let err = opts.as_model(&args).unwrap_err();
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
        let model = opts.as_model(&args).unwrap();
        assert_eq!(model.fields.len(), 1);
        assert_eq!(model.fields[0].primary_key, true);
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
        let err = opts.as_model(&args).unwrap_err();
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
        let err = opts.as_model(&args).unwrap_err();
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
        let field = field_opts.as_field();
        assert_eq!(field.field_name.to_string(), "name");
        assert_eq!(field.column_name, "name");
        assert_eq!(field.ty, parse_quote!(String));
        assert!(field.unique);
    }
}
