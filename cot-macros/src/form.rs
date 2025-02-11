use std::collections::HashMap;

use darling::{FromDeriveInput, FromField};
use heck::ToTitleCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};

use crate::cot_ident;

pub(super) fn impl_form_for_struct(ast: &syn::DeriveInput) -> TokenStream {
    let opts = match FormOpts::from_derive_input(ast) {
        Ok(val) => val,
        Err(err) => {
            return err.write_errors();
        }
    };

    let mut builder = opts.as_form_derive_builder();
    for field in opts.fields() {
        builder.push_field(field);
    }

    quote!(#builder)
}

#[derive(Debug, FromDeriveInput)]
#[darling(forward_attrs(allow, doc, cfg), supports(struct_named))]
struct FormOpts {
    ident: syn::Ident,
    data: darling::ast::Data<darling::util::Ignored, Field>,
}

impl FormOpts {
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

    fn as_form_derive_builder(&self) -> FormDeriveBuilder {
        FormDeriveBuilder {
            name: self.ident.clone(),
            context_struct_name: format_ident!("{}Context", self.ident),
            context_struct_errors_name: format_ident!("{}ContextErrors", self.ident),
            fields_as_struct_fields: Vec::with_capacity(self.field_count()),
            fields_as_struct_fields_new: Vec::with_capacity(self.field_count()),
            fields_as_context_from_request: Vec::with_capacity(self.field_count()),
            fields_as_from_context_vars: Vec::with_capacity(self.field_count()),
            fields_as_from_context: Vec::with_capacity(self.field_count()),
            fields_as_to_context: Vec::with_capacity(self.field_count()),
            fields_as_errors: Vec::with_capacity(self.field_count()),
            fields_as_errors_for: Vec::with_capacity(self.field_count()),
            fields_as_errors_for_mut: Vec::with_capacity(self.field_count()),
            fields_as_has_errors: Vec::with_capacity(self.field_count()),
            fields_as_dyn_field_ref: Vec::with_capacity(self.field_count()),
            fields_as_display: Vec::with_capacity(self.field_count()),
            fields_as_display_trait_bound: Vec::with_capacity(self.field_count()),
        }
    }
}

#[derive(Debug, Clone, FromField)]
#[darling(attributes(form))]
struct Field {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    opt: Option<HashMap<syn::Ident, syn::Expr>>,
}

#[derive(Debug)]
struct FormDeriveBuilder {
    name: syn::Ident,
    context_struct_name: syn::Ident,
    context_struct_errors_name: syn::Ident,
    fields_as_struct_fields: Vec<TokenStream>,
    fields_as_struct_fields_new: Vec<TokenStream>,
    fields_as_context_from_request: Vec<TokenStream>,
    fields_as_from_context_vars: Vec<TokenStream>,
    fields_as_from_context: Vec<TokenStream>,
    fields_as_to_context: Vec<TokenStream>,
    fields_as_errors: Vec<TokenStream>,
    fields_as_errors_for: Vec<TokenStream>,
    fields_as_errors_for_mut: Vec<TokenStream>,
    fields_as_has_errors: Vec<TokenStream>,
    fields_as_dyn_field_ref: Vec<TokenStream>,
    fields_as_display: Vec<TokenStream>,
    fields_as_display_trait_bound: Vec<TokenStream>,
}

impl ToTokens for FormDeriveBuilder {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let form_impl = self.build_form_impl();
        let form_context_impl = self.build_form_context_impl();
        let errors_struct = self.build_errors_struct();

        let new_tokens = quote! {
            const _: () = {
                #form_impl
                #form_context_impl
                #errors_struct
            };
        };

        new_tokens.to_tokens(tokens);
    }
}

impl FormDeriveBuilder {
    fn push_field(&mut self, field: &Field) {
        let crate_ident = cot_ident();
        let field_ident = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let opt = &field.opt;

        let name = field_ident.to_string().to_title_case();

        self.fields_as_struct_fields
            .push(quote!(#field_ident: <#ty as #crate_ident::form::AsFormField>::Type));

        self.fields_as_struct_fields_new.push({
            let custom_options_setters: Vec<_> = if let Some(opt) = opt {
                opt.iter()
                    .map(|(key, value)| quote!(custom_options.#key = Some(#value)))
                    .collect()
            } else {
                Vec::new()
            };
            quote!(#field_ident: {
                let options = #crate_ident::form::FormFieldOptions {
                    id: stringify!(#field_ident).to_owned(),
                    name: #name.to_owned(),
                    required: true,
                };
                type Field = <#ty as #crate_ident::form::AsFormField>::Type;
                type CustomOptions = <Field as #crate_ident::form::FormField>::CustomOptions;
                let mut custom_options: CustomOptions = ::core::default::Default::default();
                #( #custom_options_setters; )*
                <#ty as #crate_ident::form::AsFormField>::new_field(options, custom_options)
            })
        });

        self.fields_as_context_from_request
            .push(quote!(stringify!(#field_ident) => {
                #crate_ident::form::FormField::set_value(&mut self.#field_ident, value)
            }));

        let val_ident = format_ident!("val_{}", field_ident);
        self.fields_as_from_context_vars.push(quote! {
            let #val_ident = <#ty as #crate_ident::form::AsFormField>::clean_value(&context.#field_ident).map_err(|error| {
                context.add_error(#crate_ident::form::FormErrorTarget::Field(stringify!(#field_ident)), error);
            })
        });
        self.fields_as_from_context.push(
            quote!(#field_ident: #val_ident.expect("Errors should have been returned by now")),
        );
        self.fields_as_to_context
            .push(quote!(context.#field_ident.set_value(::std::borrow::Cow::Owned(self.#field_ident.to_field_value()))));

        self.fields_as_errors
            .push(quote!(#field_ident: Vec<#crate_ident::form::FormFieldValidationError>));

        self.fields_as_errors_for
            .push(quote!(stringify!(#field_ident) => self.__errors.#field_ident.as_slice()));

        self.fields_as_errors_for_mut
            .push(quote!(stringify!(#field_ident) => self.__errors.#field_ident.as_mut()));

        self.fields_as_has_errors
            .push(quote!(!self.__errors.#field_ident.is_empty()));

        self.fields_as_dyn_field_ref
            .push(quote!(&self.#field_ident as &dyn #crate_ident::form::DynFormField));

        self.fields_as_display
            .push(quote!(::core::fmt::Display::fmt(&self.#field_ident, f)?));

        self.fields_as_display_trait_bound
            .push(quote!(&'dummy <#ty as #crate_ident::form::AsFormField>::Type: ::core::fmt::Display + #crate_ident::__private::rinja::filters::HtmlSafe));
    }

    fn build_form_impl(&self) -> TokenStream {
        let crate_ident = cot_ident();
        let name = &self.name;
        let context_struct_name = &self.context_struct_name;
        let fields_as_from_context_vars = &self.fields_as_from_context_vars;
        let fields_as_from_context = &self.fields_as_from_context;
        let fields_as_to_context = &self.fields_as_to_context;

        quote! {
            #[#crate_ident::__private::async_trait]
            #[automatically_derived]
            impl #crate_ident::form::Form for #name {
                type Context = #context_struct_name;

                async fn from_request(
                    request: &mut #crate_ident::request::Request
                ) -> ::core::result::Result<#crate_ident::form::FormResult<Self>, #crate_ident::form::FormError> {
                    let mut context = <Self as #crate_ident::form::Form>::build_context(request).await?;

                    use #crate_ident::form::FormContext;
                    #( #fields_as_from_context_vars; )*

                    if context.has_errors() {
                        Ok(#crate_ident::form::FormResult::ValidationError(context))
                    } else {
                        Ok(#crate_ident::form::FormResult::Ok(Self {
                            #( #fields_as_from_context, )*
                        }))
                    }
                }

                fn to_context(
                    &self
                ) -> Self::Context {
                    use #crate_ident::form::FormContext;
                    use #crate_ident::form::AsFormField;
                    use #crate_ident::form::FormField;

                    let mut context = <Self as #crate_ident::form::Form>::Context::new();
                    #( #fields_as_to_context; )*
                    context
                }
            }
        }
    }

    fn build_form_context_impl(&self) -> TokenStream {
        let crate_ident = cot_ident();

        let context_struct_name = &self.context_struct_name;
        let context_struct_errors_name = &self.context_struct_errors_name;

        let fields_as_struct_fields = &self.fields_as_struct_fields;
        let fields_as_struct_fields_new = &self.fields_as_struct_fields_new;
        let fields_as_context_from_request = &self.fields_as_context_from_request;
        let fields_as_errors_for = &self.fields_as_errors_for;
        let fields_as_errors_for_mut = &self.fields_as_errors_for_mut;
        let fields_as_has_errors = &self.fields_as_has_errors;
        let fields_as_dyn_field_ref = &self.fields_as_dyn_field_ref;
        let fields_as_display = &self.fields_as_display;

        // <'dummy> is here because we can't directly create trivial constraints in
        // where clauses
        // see https://github.com/rust-lang/rust/issues/48214 for details
        // and the following comment for the details on the workaround being used here:
        // https://github.com/rust-lang/rust/issues/48214#issuecomment-2557829956
        let fields_as_display_trait_bound = &self.fields_as_display_trait_bound;
        let display_where_clause = if fields_as_display_trait_bound.is_empty() {
            quote! {}
        } else {
            quote! {
                where #( #fields_as_display_trait_bound, )*
            }
        };
        let display_dummy_lifetime_decl = if fields_as_display_trait_bound.is_empty() {
            quote! {}
        } else {
            quote! { <'dummy> }
        };

        quote! {
            #[derive(::core::fmt::Debug)]
            pub struct #context_struct_name {
                __errors: #context_struct_errors_name,
                #( #fields_as_struct_fields, )*
            }

            #[automatically_derived]
            impl #crate_ident::form::FormContext for #context_struct_name {
                fn new() -> Self {
                    Self {
                        __errors: ::core::default::Default::default(),
                        #( #fields_as_struct_fields_new, )*
                    }
                }

                fn fields(
                    &self,
                ) -> ::std::boxed::Box<dyn ::core::iter::DoubleEndedIterator<
                    Item = &dyn ::cot::form::DynFormField,
                > + '_> {
                    Box::new([#( #fields_as_dyn_field_ref, )*].into_iter())
                }

                fn set_value(
                    &mut self,
                    field_id: &str,
                    value: ::std::borrow::Cow<str>,
                ) -> ::core::result::Result<(), #crate_ident::form::FormFieldValidationError> {
                    match field_id {
                        #( #fields_as_context_from_request, )*
                        _ => {}
                    }
                    Ok(())
                }

                fn errors_for(
                    &self,
                    target: #crate_ident::form::FormErrorTarget
                ) -> &[#crate_ident::form::FormFieldValidationError] {
                    match target {
                        #crate_ident::form::FormErrorTarget::Field(field_id) => {
                            match field_id {
                                #( #fields_as_errors_for, )*
                                _ => {
                                    panic!("Unknown field name passed to get_errors: `{}`", field_id);
                                }
                            }
                        }
                        #crate_ident::form::FormErrorTarget::Form => {
                            self.__errors.__form.as_slice()
                        }
                    }
                }

                fn errors_for_mut(
                    &mut self,
                    target: #crate_ident::form::FormErrorTarget
                ) -> &mut Vec<#crate_ident::form::FormFieldValidationError> {
                    match target {
                        #crate_ident::form::FormErrorTarget::Field(field_id) => {
                            match field_id {
                                #( #fields_as_errors_for_mut, )*
                                _ => {
                                    panic!("Unknown field name passed to get_errors_mut: `{}`", field_id);
                                }
                            }
                        }
                        #crate_ident::form::FormErrorTarget::Form => {
                            self.__errors.__form.as_mut()
                        }
                    }
                }

                fn has_errors(&self) -> bool {
                    !self.__errors.__form.is_empty() #( || #fields_as_has_errors )*
                }
            }

            #[automatically_derived]
            impl #display_dummy_lifetime_decl ::core::fmt::Display for #context_struct_name #display_where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    #( #fields_as_display; )*

                    Ok(())
                }
            }

            #[automatically_derived]
            impl #display_dummy_lifetime_decl #crate_ident::__private::rinja::filters::HtmlSafe for #context_struct_name #display_where_clause {}
        }
    }

    fn build_errors_struct(&self) -> TokenStream {
        let crate_ident = cot_ident();
        let context_struct_errors_name = &self.context_struct_errors_name;
        let fields_as_errors = &self.fields_as_errors;

        quote! {
            #[derive(::core::fmt::Debug, ::core::default::Default)]
            struct #context_struct_errors_name {
                __form: Vec<#crate_ident::form::FormFieldValidationError>,
                #( #fields_as_errors, )*
            }
        }
    }
}
