mod form;

use proc_macro::TokenStream;
use proc_macro_crate::crate_name;
use quote::quote;
use syn::parse_macro_input;

use crate::form::impl_form_for_struct;

/// Derive the [`Form`] trait for a struct.
///
/// This macro will generate an implementation of the [`Form`] trait for the
/// given named struct. Note that all the fields of the struct **must**
/// implement the [`AsFormField`] trait.
///
/// [`Form`]: trait.Form.html
/// [`AsFormField`]: trait.AsFormField.html
#[proc_macro_derive(Form, attributes(form))]
pub fn derive_form(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    let token_stream = impl_form_for_struct(ast);
    token_stream.into()
}

pub(crate) fn flareon_ident() -> proc_macro2::TokenStream {
    let flareon_crate = crate_name("flareon").expect("flareon is not present in `Cargo.toml`");
    match flareon_crate {
        proc_macro_crate::FoundCrate::Itself => {
            quote! { ::flareon }
        }
        proc_macro_crate::FoundCrate::Name(name) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote! { ::#ident }
        }
    }
}
