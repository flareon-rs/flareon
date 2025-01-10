use darling::ast::NestedMeta;
use darling::FromMeta;
use flareon_codegen::model::{Field, Model, ModelArgs, ModelOpts};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, ToTokens, TokenStreamExt};
use syn::punctuated::Punctuated;
use syn::Token;

use crate::flareon_ident;

#[must_use]
pub(super) fn impl_model_for_struct(
    args: &[NestedMeta],
    ast: &mut syn::DeriveInput,
) -> TokenStream {
    let args = match ModelArgs::from_list(args) {
        Ok(v) => v,
        Err(e) => {
            return e.write_errors();
        }
    };

    let opts = match ModelOpts::new_from_derive_input(ast) {
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

    let attrs = &ast.attrs;
    let vis = &ast.vis;
    let ident = &ast.ident;

    // Filter out our helper attributes so they don't get passed to the struct
    let fields = match &mut ast.data {
        syn::Data::Struct(data) => &mut data.fields,
        _ => panic!("Only structs are supported"),
    };
    let fields = remove_helper_field_attributes(fields);

    quote!(
        #(#attrs)*
        #vis struct #ident {
            #fields
        }
        #builder
    )
}

fn remove_helper_field_attributes(fields: &mut syn::Fields) -> &Punctuated<syn::Field, Token![,]> {
    match fields {
        syn::Fields::Named(fields) => {
            for field in &mut fields.named {
                field.attrs.retain(|a| !a.path().is_ident("model"));
            }
            &fields.named
        }
        _ => panic!("Only named fields are supported"),
    }
}

#[derive(Debug)]
struct ModelBuilder {
    name: Ident,
    table_name: String,
    pk_field: Field,
    fields_struct_name: Ident,
    fields_as_columns: Vec<TokenStream>,
    fields_as_from_db: Vec<TokenStream>,
    fields_as_update_from_db: Vec<TokenStream>,
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
            pk_field: model.pk_field.clone(),
            fields_struct_name: format_ident!("{}Fields", model.name),
            fields_as_columns: Vec::with_capacity(field_count),
            fields_as_from_db: Vec::with_capacity(field_count),
            fields_as_update_from_db: Vec::with_capacity(field_count),
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

        {
            let field_as_column = quote!(#orm_ident::Column::new(
                #orm_ident::Identifier::new(#column_name)
            ));
            self.fields_as_columns.push(field_as_column);
        }

        self.fields_as_from_db.push(quote!(
            #name: db_row.get::<#ty>(#index)?
        ));

        self.fields_as_update_from_db.push(quote!(
            #index => { self.#name = db_row.get::<#ty>(row_field_id)?; }
        ));

        self.fields_as_get_values.push(quote!(
            #index => &self.#name as &dyn #orm_ident::ToDbFieldValue
        ));

        self.fields_as_field_refs.push(quote!(
            pub const #name: #orm_ident::query::FieldRef<#ty> =
                #orm_ident::query::FieldRef::<#ty>::new(#orm_ident::Identifier::new(#column_name));
        ));
    }

    #[must_use]
    fn build_model_impl(&self) -> TokenStream {
        let crate_ident = flareon_ident();
        let orm_ident = orm_ident();

        let name = &self.name;
        let table_name = &self.table_name;
        let fields_struct_name = &self.fields_struct_name;
        let fields_as_columns = &self.fields_as_columns;
        let pk_field_name = &self.pk_field.field_name;
        let pk_column_name = &self.pk_field.column_name;
        let pk_type = &self.pk_field.ty;
        let fields_as_from_db = &self.fields_as_from_db;
        let fields_as_update_from_db = &self.fields_as_update_from_db;
        let fields_as_get_values = &self.fields_as_get_values;

        quote! {
            #[#crate_ident::__private::async_trait]
            #[automatically_derived]
            impl #orm_ident::Model for #name {
                type Fields = #fields_struct_name;
                type PrimaryKey = #pk_type;

                const COLUMNS: &'static [#orm_ident::Column] = &[
                    #(#fields_as_columns,)*
                ];
                const TABLE_NAME: #orm_ident::Identifier = #orm_ident::Identifier::new(#table_name);
                const PRIMARY_KEY_NAME: #orm_ident::Identifier = #orm_ident::Identifier::new(#pk_column_name);

                fn primary_key(&self) -> &Self::PrimaryKey {
                    &self.#pk_field_name
                }

                fn set_primary_key(&mut self, primary_key: Self::PrimaryKey) {
                    self.#pk_field_name = primary_key;
                }

                fn from_db(db_row: #orm_ident::Row) -> #orm_ident::Result<Self> {
                    Ok(Self {
                        #(#fields_as_from_db,)*
                    })
                }

                fn update_from_db(&mut self, db_row: #orm_ident::Row, columns: &[usize]) -> #orm_ident::Result<()> {
                    for (row_field_id, column_id) in columns.into_iter().enumerate() {
                        match *column_id {
                            #(#fields_as_update_from_db,)*
                            _ => panic!("Unknown column index: {}", column_id),
                        }
                    }

                    Ok(())
                }

                fn get_values(&self, columns: &[usize]) -> Vec<&dyn #orm_ident::ToDbFieldValue> {
                    columns
                        .iter()
                        .map(|&column| match column {
                            #(#fields_as_get_values,)*
                            _ => panic!("Unknown column index: {}", column),
                        })
                        .collect()
                }

                async fn get_by_primary_key<DB: #orm_ident::DatabaseBackend>(
                    db: &DB,
                    pk: Self::PrimaryKey,
                ) -> #orm_ident::Result<Option<Self>> {
                    #orm_ident::query!(Self, $#pk_field_name == pk)
                        .get(db)
                        .await
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
