use convert_case::{Case, Casing};
use darling::{FromDeriveInput, FromField, FromMeta};

#[derive(Debug, FromMeta)]
pub struct ModelArgs {
    #[darling(default)]
    pub model_type: ModelType,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default, FromMeta)]
pub enum ModelType {
    #[default]
    Application,
    Migration,
}

#[derive(Debug, Clone, FromDeriveInput)]
#[darling(forward_attrs(allow, doc, cfg), supports(struct_named))]
pub struct ModelOpts {
    pub ident: syn::Ident,
    pub data: darling::ast::Data<darling::util::Ignored, FieldOpts>,
}

impl ModelOpts {
    #[must_use]
    pub fn fields(&self) -> Vec<&FieldOpts> {
        self.data
            .as_ref()
            .take_struct()
            .expect("Only structs are supported")
            .fields
    }

    #[must_use]
    pub fn field_count(&self) -> usize {
        self.fields().len()
    }
}

impl From<ModelOpts> for Model {
    fn from(opts: ModelOpts) -> Self {
        let table_name = opts.ident.to_string().to_case(Case::Snake);
        let fields = opts.fields().iter().map(|field| field.as_field()).collect();

        Self {
            name: opts.ident.clone(),
            table_name,
            fields,
        }
    }
}

#[derive(Debug, Clone, FromField)]
#[darling(attributes(form))]
pub struct FieldOpts {
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
}

impl FieldOpts {
    #[must_use]
    pub fn as_field(&self) -> Field {
        let name = self.ident.as_ref().unwrap();
        let column_name = name.to_string().to_case(Case::Snake);
        let is_auto = column_name == "id";

        Field {
            field_name: name.clone(),
            column_name,
            ty: self.ty.clone(),
            auto_value: is_auto,
            null: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub name: syn::Ident,
    pub table_name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    pub field_name: syn::Ident,
    pub column_name: String,
    pub ty: syn::Type,
    pub auto_value: bool,
    pub null: bool,
}
