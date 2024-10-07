use std::collections::HashMap;

use darling::{FromDeriveInput, FromField};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens, TokenStreamExt};

use crate::flareon_ident;

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
            fields_as_errors: Vec::with_capacity(self.field_count()),
            fields_as_errors_for: Vec::with_capacity(self.field_count()),
            fields_as_errors_for_mut: Vec::with_capacity(self.field_count()),
            fields_as_has_errors: Vec::with_capacity(self.field_count()),
            fields_as_dyn_field_ref: Vec::with_capacity(self.field_count()),
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
    fields_as_errors: Vec<TokenStream>,
    fields_as_errors_for: Vec<TokenStream>,
    fields_as_errors_for_mut: Vec<TokenStream>,
    fields_as_has_errors: Vec<TokenStream>,
    fields_as_dyn_field_ref: Vec<TokenStream>,
}

impl ToTokens for FormDeriveBuilder {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append_all(self.build_form_impl());
        tokens.append_all(self.build_form_context_impl());
        tokens.append_all(self.build_errors_struct());
    }
}

impl FormDeriveBuilder {
    fn push_field(&mut self, field: &Field) {
        let crate_ident = flareon_ident();
        let name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let opt = &field.opt;

        self.fields_as_struct_fields
            .push(quote!(#name: <#ty as #crate_ident::forms::AsFormField>::Type));

        self.fields_as_struct_fields_new.push({
            let custom_options_setters: Vec<_> = if let Some(opt) = opt {
                opt.iter()
                    .map(|(key, value)| quote!(custom_options.#key = Some(#value)))
                    .collect()
            } else {
                Vec::new()
            };
            quote!(#name: {
                let options = #crate_ident::forms::FormFieldOptions {
                    id: stringify!(#name).to_owned(),
                    required: true,
                };
                type Field = <#ty as #crate_ident::forms::AsFormField>::Type;
                type CustomOptions = <Field as #crate_ident::forms::FormField>::CustomOptions;
                let mut custom_options: CustomOptions = ::core::default::Default::default();
                #( #custom_options_setters; )*
                <#ty as #crate_ident::forms::AsFormField>::new_field(options, custom_options)
            })
        });

        self.fields_as_context_from_request
            .push(quote!(stringify!(#name) => {
                #crate_ident::forms::FormField::set_value(&mut self.#name, value)
            }));

        let val_ident = format_ident!("val_{}", name);
        self.fields_as_from_context_vars.push(quote! {
            let #val_ident = <#ty as #crate_ident::forms::AsFormField>::clean_value(&context.#name).map_err(|error| {
                context.add_error(#crate_ident::forms::FormErrorTarget::Field(stringify!(#name)), error);
            })
        });
        self.fields_as_from_context
            .push(quote!(#name: #val_ident.expect("Errors should have been returned by now")));

        self.fields_as_errors
            .push(quote!(#name: Vec<#crate_ident::forms::FormFieldValidationError>));

        self.fields_as_errors_for
            .push(quote!(stringify!(#name) => self.__errors.#name.as_slice()));

        self.fields_as_errors_for_mut
            .push(quote!(stringify!(#name) => self.__errors.#name.as_mut()));

        self.fields_as_has_errors
            .push(quote!(!self.__errors.#name.is_empty()));

        self.fields_as_dyn_field_ref
            .push(quote!(&self.#name as &dyn #crate_ident::forms::DynFormField));
    }

    fn build_form_impl(&self) -> TokenStream {
        let crate_ident = flareon_ident();
        let name = &self.name;
        let context_struct_name = &self.context_struct_name;
        let fields_as_from_context_vars = &self.fields_as_from_context_vars;
        let fields_as_from_context = &self.fields_as_from_context;

        quote! {
            #[#crate_ident::__private::async_trait]
            #[automatically_derived]
            impl #crate_ident::forms::Form for #name {
                type Context = #context_struct_name;

                async fn from_request(
                    request: &mut #crate_ident::request::Request
                ) -> ::core::result::Result<#crate_ident::forms::FormResult<Self>, #crate_ident::forms::FormError> {
                    let mut context = <Self as #crate_ident::forms::Form>::build_context(request).await?;

                    use #crate_ident::forms::FormContext;
                    #( #fields_as_from_context_vars; )*

                    if context.has_errors() {
                        Ok(#crate_ident::forms::FormResult::ValidationError(context))
                    } else {
                        Ok(#crate_ident::forms::FormResult::Ok(Self {
                            #( #fields_as_from_context, )*
                        }))
                    }
                }
            }
        }
    }

    fn build_form_context_impl(&self) -> TokenStream {
        let crate_ident = flareon_ident();

        let context_struct_name = &self.context_struct_name;
        let context_struct_errors_name = &self.context_struct_errors_name;

        let fields_as_struct_fields = &self.fields_as_struct_fields;
        let fields_as_struct_fields_new = &self.fields_as_struct_fields_new;
        let fields_as_context_from_request = &self.fields_as_context_from_request;
        let fields_as_errors_for = &self.fields_as_errors_for;
        let fields_as_errors_for_mut = &self.fields_as_errors_for_mut;
        let fields_as_has_errors = &self.fields_as_has_errors;
        let fields_as_dyn_field_ref = &self.fields_as_dyn_field_ref;

        quote! {
            #[derive(::core::fmt::Debug)]
            struct #context_struct_name {
                __errors: #context_struct_errors_name,
                #( #fields_as_struct_fields, )*
            }

            #[automatically_derived]
            impl #crate_ident::forms::FormContext for #context_struct_name {
                fn new() -> Self {
                    Self {
                        __errors: ::core::default::Default::default(),
                        #( #fields_as_struct_fields_new, )*
                    }
                }

                fn fields(
                    &self,
                ) -> impl ::core::iter::DoubleEndedIterator<
                    Item = &dyn ::flareon::forms::DynFormField,
                > + ::core::iter::ExactSizeIterator + '_ {
                    [#( #fields_as_dyn_field_ref, )*].into_iter()
                }

                fn set_value(
                    &mut self,
                    field_id: &str,
                    value: ::std::borrow::Cow<str>,
                ) -> ::core::result::Result<(), #crate_ident::forms::FormFieldValidationError> {
                    match field_id {
                        #( #fields_as_context_from_request, )*
                        _ => {}
                    }
                    Ok(())
                }

                fn errors_for(
                    &self,
                    target: #crate_ident::forms::FormErrorTarget
                ) -> &[#crate_ident::forms::FormFieldValidationError] {
                    match target {
                        #crate_ident::forms::FormErrorTarget::Field(field_id) => {
                            match field_id {
                                #( #fields_as_errors_for, )*
                                _ => {
                                    panic!("Unknown field name passed to get_errors: `{}`", field_id);
                                }
                            }
                        }
                        #crate_ident::forms::FormErrorTarget::Form => {
                            self.__errors.__form.as_slice()
                        }
                    }
                }

                fn errors_for_mut(
                    &mut self,
                    target: #crate_ident::forms::FormErrorTarget
                ) -> &mut Vec<#crate_ident::forms::FormFieldValidationError> {
                    match target {
                        #crate_ident::forms::FormErrorTarget::Field(field_id) => {
                            match field_id {
                                #( #fields_as_errors_for_mut, )*
                                _ => {
                                    panic!("Unknown field name passed to get_errors_mut: `{}`", field_id);
                                }
                            }
                        }
                        #crate_ident::forms::FormErrorTarget::Form => {
                            self.__errors.__form.as_mut()
                        }
                    }
                }

                fn has_errors(&self) -> bool {
                    !self.__errors.__form.is_empty() #( || #fields_as_has_errors )*
                }
            }
        }
    }

    fn build_errors_struct(&self) -> TokenStream {
        let crate_ident = flareon_ident();
        let context_struct_errors_name = &self.context_struct_errors_name;
        let fields_as_errors = &self.fields_as_errors;

        quote! {
            #[derive(::core::fmt::Debug, ::core::default::Default)]
            struct #context_struct_errors_name {
                __form: Vec<#crate_ident::forms::FormFieldValidationError>,
                #( #fields_as_errors, )*
            }
        }
    }
}
