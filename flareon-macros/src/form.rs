use darling::{FromDeriveInput, FromField, FromMeta};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, ToTokens, TokenStreamExt};

pub fn form_for_struct(ast: syn::DeriveInput) -> proc_macro2::TokenStream {
    let opts = match FormOpts::from_derive_input(&ast) {
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
#[darling(attributes(my_crate), forward_attrs(allow, doc, cfg))]
pub struct FormOpts {
    ident: syn::Ident,
    attrs: Vec<syn::Attribute>,
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
            context_struct_field_iterator_name: format_ident!("{}ContextFieldIterator", self.ident),
            fields_as_struct_fields: Vec::with_capacity(self.field_count()),
            fields_as_struct_fields_new: Vec::with_capacity(self.field_count()),
            fields_as_context_from_request: Vec::with_capacity(self.field_count()),
            fields_as_from_context: Vec::with_capacity(self.field_count()),
            fields_as_errors: Vec::with_capacity(self.field_count()),
            fields_as_get_errors: Vec::with_capacity(self.field_count()),
            fields_as_get_errors_mut: Vec::with_capacity(self.field_count()),
            fields_as_iterator_next: Vec::with_capacity(self.field_count()),
        }
    }
}

#[derive(Debug, Clone, FromField)]
pub struct Field {
    ident: Option<syn::Ident>,
    ty: syn::Type,
}

#[derive(Debug)]
struct FormDeriveBuilder {
    name: Ident,
    context_struct_name: Ident,
    context_struct_errors_name: Ident,
    context_struct_field_iterator_name: Ident,
    fields_as_struct_fields: Vec<TokenStream>,
    fields_as_struct_fields_new: Vec<TokenStream>,
    fields_as_context_from_request: Vec<TokenStream>,
    fields_as_from_context: Vec<TokenStream>,
    fields_as_errors: Vec<TokenStream>,
    fields_as_get_errors: Vec<TokenStream>,
    fields_as_get_errors_mut: Vec<TokenStream>,
    fields_as_iterator_next: Vec<TokenStream>,
}

impl ToTokens for FormDeriveBuilder {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append_all(self.build_form_impl());
        tokens.append_all(self.build_form_context_impl());
        tokens.append_all(self.build_errors_struct());
        tokens.append_all(self.build_context_field_iterator_impl());
    }
}

impl FormDeriveBuilder {
    fn push_field(&mut self, field: &Field) {
        let name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let index = self.fields_as_struct_fields.len();

        self.fields_as_struct_fields
            .push(quote!(#name: <#ty as ::flareon::forms::AsFormField>::Type));

        self.fields_as_struct_fields_new.push(quote!(#name: {
            let options = ::flareon::forms::FormFieldOptions {
                id: stringify!(#name).to_owned(),
            };
            <#ty as ::flareon::forms::AsFormField>::with_options(options)
        }));

        self.fields_as_context_from_request
            .push(quote!(stringify!(#name) => {
                ::flareon::forms::FormField::set_value(&mut self.#name, value)
            }));

        self.fields_as_from_context.push(quote!(#name: <#ty as ::flareon::forms::AsFormField>::clean_value(&context.#name).unwrap()));

        self.fields_as_errors
            .push(quote!(#name: Vec<::flareon::forms::FormFieldValidationError>));

        self.fields_as_get_errors
            .push(quote!(stringify!(#name) => self.__errors.#name.as_slice()));

        self.fields_as_get_errors_mut
            .push(quote!(stringify!(#name) => self.__errors.#name.as_mut()));

        self.fields_as_iterator_next.push(
            quote!(#index => Some(&self.context.#name as &'a dyn ::flareon::forms::FormField)),
        );
    }

    fn build_form_impl(&self) -> TokenStream {
        let name = &self.name;
        let context_struct_name = &self.context_struct_name;
        let fields_as_from_context = &self.fields_as_from_context;

        quote! {
            #[::flareon::private::async_trait]
            #[automatically_derived]
            impl ::flareon::forms::Form for #name {
                type Context = #context_struct_name;

                async fn from_request(request: &mut ::flareon::request::Request) -> Result<Self, ::flareon::forms::FormError<Self>>
                {
                    let mut context = <Self as ::flareon::forms::Form>::build_context(request).await?;

                    Ok(Self {
                        #( #fields_as_from_context, )*
                    })
                }
            }
        }
    }

    fn build_form_context_impl(&self) -> TokenStream {
        let context_struct_name = &self.context_struct_name;
        let context_struct_errors_name = &self.context_struct_errors_name;
        let context_struct_field_iterator_name = &self.context_struct_field_iterator_name;

        let fields_as_struct_fields = &self.fields_as_struct_fields;
        let fields_as_struct_fields_new = &self.fields_as_struct_fields_new;
        let fields_as_context_from_request = &self.fields_as_context_from_request;
        let fields_as_get_errors = &self.fields_as_get_errors;
        let fields_as_get_errors_mut = &self.fields_as_get_errors_mut;

        quote! {
            #[derive(::core::fmt::Debug)]
            struct #context_struct_name {
                __errors: #context_struct_errors_name,
                #( #fields_as_struct_fields, )*
            }

            #[automatically_derived]
            impl ::flareon::forms::FormContext for #context_struct_name {
                fn new() -> Self {
                    Self {
                        __errors: Default::default(),
                        #( #fields_as_struct_fields_new, )*
                    }
                }

                fn fields(&self) -> impl Iterator<Item = &dyn ::flareon::forms::FormField> + '_ {
                    #context_struct_field_iterator_name {
                        context: self,
                        index: 0,
                    }
                }

                fn set_value(
                    &mut self,
                    field_id: &str,
                    value: ::std::borrow::Cow<str>,
                ) -> Result<(), ::flareon::forms::FormFieldValidationError> {
                    match field_id {
                        #( #fields_as_context_from_request, )*
                        _ => {}
                    }
                    Ok(())
                }

                fn get_errors(&self, target: ::flareon::forms::FormErrorTarget) -> &[::flareon::forms::FormFieldValidationError] {
                    match target {
                        ::flareon::forms::FormErrorTarget::Field(field_id) => {
                            match field_id {
                                #( #fields_as_get_errors, )*
                                _ => {
                                    panic!("Unknown field name passed to get_errors: `{}`", field_id);
                                }
                            }
                        }
                        ::flareon::forms::FormErrorTarget::Form => {
                            self.__errors.__form.as_slice()
                        }
                    }
                }

                fn get_errors_mut(&mut self, target: ::flareon::forms::FormErrorTarget) -> &mut Vec<::flareon::forms::FormFieldValidationError> {
                    match target {
                        ::flareon::forms::FormErrorTarget::Field(field_id) => {
                            match field_id {
                                #( #fields_as_get_errors_mut, )*
                                _ => {
                                    panic!("Unknown field name passed to get_errors_mut: `{}`", field_id);
                                }
                            }
                        }
                        ::flareon::forms::FormErrorTarget::Form => {
                            self.__errors.__form.as_mut()
                        }
                    }
                }
            }
        }
    }

    fn build_errors_struct(&self) -> TokenStream {
        let context_struct_errors_name = &self.context_struct_errors_name;
        let fields_as_errors = &self.fields_as_errors;

        quote! {
            #[derive(::core::fmt::Debug, ::core::default::Default)]
            struct #context_struct_errors_name {
                __form: Vec<::flareon::forms::FormFieldValidationError>,
                #( #fields_as_errors, )*
            }
        }
    }

    fn build_context_field_iterator_impl(&self) -> TokenStream {
        let context_struct_name = &self.context_struct_name;
        let context_struct_field_iterator_name = &self.context_struct_field_iterator_name;
        let fields_as_iterator_next = &self.fields_as_iterator_next;

        quote! {
            #[derive(::core::fmt::Debug)]
            struct #context_struct_field_iterator_name<'a> {
                context: &'a #context_struct_name,
                index: usize,
            }

            #[automatically_derived]
            impl<'a> Iterator for #context_struct_field_iterator_name<'a> {
                type Item = &'a dyn ::flareon::forms::FormField;

                fn next(&mut self) -> Option<Self::Item> {
                    let result = match self.index {
                        #( #fields_as_iterator_next, )*
                        _ => None,
                    };

                    if result.is_some() {
                        self.index += 1;
                    } else {
                        self.index = 0;
                    }

                    result
                }
            }
        }
    }
}
