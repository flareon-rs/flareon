use convert_case::{Case, Casing};
use darling::ast::NestedMeta;
use darling::{FromDeriveInput, FromField};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, ToTokens, TokenStreamExt};

use crate::flareon_ident;

pub fn impl_model_for_struct(_args: Vec<NestedMeta>, ast: syn::DeriveInput) -> TokenStream {
    let opts = match ModelOpts::from_derive_input(&ast) {
        Ok(val) => val,
        Err(err) => {
            return err.write_errors();
        }
    };

    let mut builder = opts.as_model_builder();
    for field in opts.fields() {
        builder.push_field(field);
    }

    quote!(#ast #builder)
}

#[derive(Debug, FromDeriveInput)]
#[darling(forward_attrs(allow, doc, cfg), supports(struct_named))]
struct ModelOpts {
    ident: syn::Ident,
    data: darling::ast::Data<darling::util::Ignored, Field>,
}

impl ModelOpts {
    fn fields(&self) -> Vec<&Field> {
        self.data
            .as_ref()
            .take_struct()
            .expect("Only structs are supported")
            .fields
    }

    fn field_count(&self) -> usize {
        self.fields().len()
    }

    fn as_model_builder(&self) -> ModelBuilder {
        let table_name = self.ident.to_string().to_case(Case::Snake);

        ModelBuilder {
            name: self.ident.clone(),
            table_name,
            fields_struct_name: format_ident!("{}Fields", self.ident),
            fields_as_columns: Vec::with_capacity(self.field_count()),
            fields_as_from_db: Vec::with_capacity(self.field_count()),
            fields_as_get_values: Vec::with_capacity(self.field_count()),
            fields_as_field_refs: Vec::with_capacity(self.field_count()),
        }
    }
}

#[derive(Debug, Clone, FromField)]
#[darling(attributes(form))]
struct Field {
    ident: Option<syn::Ident>,
    ty: syn::Type,
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
    fn push_field(&mut self, field: &Field) {
        let orm_ident = orm_ident();

        let name = field.ident.as_ref().unwrap();
        let const_name = format_ident!("{}", name.to_string().to_case(Case::UpperSnake));
        let ty = &field.ty;
        let index = self.fields_as_columns.len();

        let column_name = name.to_string().to_case(Case::Snake);
        let is_auto = column_name == "id";

        {
            let mut field_as_column = quote!(#orm_ident::Column::new(
                #orm_ident::Identifier::new(#column_name)
            ));
            if is_auto {
                field_as_column.append_all(quote!(.auto(true)));
            }
            self.fields_as_columns.push(field_as_column);
        }

        self.fields_as_from_db.push(quote!(
            #name: db_row.get::<#ty>(#index)?
        ));

        self.fields_as_get_values.push(quote!(
            #index => &self.#name as &dyn #orm_ident::ValueRef
        ));

        self.fields_as_field_refs.push(quote!(
            pub const #const_name: #orm_ident::query::FieldRef<#ty> =
                #orm_ident::query::FieldRef::<#ty>::new(#orm_ident::Identifier::new(#column_name));
        ));
    }

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

                fn get_values(&self, columns: &[usize]) -> Vec<&dyn #orm_ident::ValueRef> {
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

    fn build_fields_struct(&self) -> TokenStream {
        let fields_struct_name = &self.fields_struct_name;
        let fields_as_field_refs = &self.fields_as_field_refs;

        quote! {
            #[derive(::core::fmt::Debug)]
            pub struct #fields_struct_name;

            impl #fields_struct_name {
                #(#fields_as_field_refs)*
            }
        }
    }
}

fn orm_ident() -> TokenStream {
    let crate_ident = flareon_ident();
    quote! { #crate_ident::db }
}
