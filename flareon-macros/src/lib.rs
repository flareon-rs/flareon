mod form;

use darling::FromDeriveInput;
use proc_macro::TokenStream;
use syn::parse_macro_input;

use crate::form::form_for_struct;

#[proc_macro_derive(Form)]
pub fn derive_form(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    let token_stream = form_for_struct(ast);
    println!("{}", token_stream);
    token_stream.into()
}
