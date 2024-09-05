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
    pub data: darling::ast::Data<darling::util::Ignored, FieldOpts>,
}

impl ModelOpts {
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
        let fields = self.fields().iter().map(|field| field.as_field()).collect();

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

        Ok(Model {
            name: self.ident.clone(),
            original_name,
            model_type: args.model_type,
            table_name,
            fields,
        })
    }
}

#[derive(Debug, Clone, FromField)]
#[darling(attributes(form))]
pub struct FieldOpts {
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
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
        // TODO define #[model(primary_key)] attribute
        let is_primary_key = column_name == "id";

        Field {
            field_name: name.clone(),
            column_name,
            ty: self.ty.clone(),
            auto_value: is_auto,
            primary_key: is_primary_key,
            null: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub name: syn::Ident,
    pub original_name: String,
    pub model_type: ModelType,
    pub table_name: String,
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
}
