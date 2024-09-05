use darling::ast::NestedMeta;
use darling::{FromDeriveInput, FromMeta};
use flareon_codegen::model::{Field, Model, ModelArgs, ModelOpts};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, ToTokens, TokenStreamExt};

use crate::flareon_ident;

#[must_use]
pub(super) fn impl_model_for_struct(args: &[NestedMeta], ast: &syn::DeriveInput) -> TokenStream {
    let args = match ModelArgs::from_list(args) {
        Ok(v) => v,
        Err(e) => {
            return e.write_errors();
        }
    };

    let opts = match ModelOpts::from_derive_input(ast) {
        Ok(val) => val,
        Err(err) => {
            return err.write_errors();
        }
    };

    let model = match opts.as_model(&args) {
        Ok(val) => val,
        Err(err) => {
            return err.to_compile_error();
        }
    };
    let builder = ModelBuilder::from_model(model);

    quote!(#ast #builder)
}

#[derive(Debug)]
struct ModelBuilder {
    name: Ident,
    table_name: String,
    fields_struct_name: Ident,
    fields_as_columns: Vec<TokenStream>,
    fields_as_from_db: Vec<TokenStream>,
    fields_as_get_values: Vec<TokenStream>,
    fields_as_field_refs: Vec<TokenStream>,
}

impl ToTokens for ModelBuilder {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append_all(self.build_model_impl());
        tokens.append_all(self.build_fields_struct());
    }
}

impl ModelBuilder {
    fn from_model(model: Model) -> Self {
        let field_count = model.field_count();
        let mut model_builder = Self {
            name: model.name.clone(),
            table_name: model.table_name,
            fields_struct_name: format_ident!("{}Fields", model.name),
            fields_as_columns: Vec::with_capacity(field_count),
            fields_as_from_db: Vec::with_capacity(field_count),
            fields_as_get_values: Vec::with_capacity(field_count),
            fields_as_field_refs: Vec::with_capacity(field_count),
        };
        for field in &model.fields {
            model_builder.push_field(field);
        }

        model_builder
    }

    fn push_field(&mut self, field: &Field) {
        let orm_ident = orm_ident();

        let name = &field.field_name;
        let ty = &field.ty;
        let index = self.fields_as_columns.len();
        let column_name = &field.column_name;
        let is_auto = field.auto_value;
        let is_null = field.null;

        {
            let mut field_as_column = quote!(#orm_ident::Column::new(
                #orm_ident::Identifier::new(#column_name)
            ));
            if is_auto {
                field_as_column.append_all(quote!(.auto()));
            }
            if is_null {
                field_as_column.append_all(quote!(.null()));
            }
            self.fields_as_columns.push(field_as_column);
        }

        self.fields_as_from_db.push(quote!(
            #name: db_row.get::<#ty>(#index)?
        ));

        self.fields_as_get_values.push(quote!(
            #index => &self.#name as &dyn #orm_ident::ToDbValue
        ));

        self.fields_as_field_refs.push(quote!(
            pub const #name: #orm_ident::query::FieldRef<#ty> =
                #orm_ident::query::FieldRef::<#ty>::new(#orm_ident::Identifier::new(#column_name));
        ));
    }

    #[must_use]
    fn build_model_impl(&self) -> TokenStream {
        let orm_ident = orm_ident();

        let name = &self.name;
        let table_name = &self.table_name;
        let fields_struct_name = &self.fields_struct_name;
        let fields_as_columns = &self.fields_as_columns;
        let fields_as_from_db = &self.fields_as_from_db;
        let fields_as_get_values = &self.fields_as_get_values;

        quote! {
            #[automatically_derived]
            impl #orm_ident::Model for #name {
                type Fields = #fields_struct_name;

                const COLUMNS: &'static [#orm_ident::Column] = &[
                    #(#fields_as_columns,)*
                ];
                const TABLE_NAME: #orm_ident::Identifier = #orm_ident::Identifier::new(#table_name);

                fn from_db(db_row: #orm_ident::Row) -> #orm_ident::Result<Self> {
                    Ok(Self {
                        #(#fields_as_from_db,)*
                    })
                }

                fn get_values(&self, columns: &[usize]) -> Vec<&dyn #orm_ident::ToDbValue> {
                    columns
                        .iter()
                        .map(|&column| match column {
                            #(#fields_as_get_values,)*
                            _ => panic!("Unknown column index: {}", column),
                        })
                        .collect()
                }
            }
        }
    }

    #[must_use]
    fn build_fields_struct(&self) -> TokenStream {
        let fields_struct_name = &self.fields_struct_name;
        let fields_as_field_refs = &self.fields_as_field_refs;

        quote! {
            #[derive(::core::fmt::Debug)]
            pub struct #fields_struct_name;

            #[allow(non_upper_case_globals)]
            impl #fields_struct_name {
                #(#fields_as_field_refs)*
            }
        }
    }
}

#[must_use]
fn orm_ident() -> TokenStream {
    let crate_ident = flareon_ident();
    quote! { #crate_ident::db }
}
