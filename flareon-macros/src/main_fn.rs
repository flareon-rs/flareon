use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

use crate::flareon_ident;

pub(super) fn fn_to_flareon_main(main_function_decl: ItemFn) -> syn::Result<TokenStream> {
    let mut new_main_decl = main_function_decl.clone();
    new_main_decl.sig.ident =
        syn::Ident::new("__flareon_main", main_function_decl.sig.ident.span());

    if !main_function_decl.sig.inputs.is_empty() {
        return Err(syn::Error::new_spanned(
            main_function_decl.sig.inputs,
            "flareon::main function must have zero arguments",
        ));
    }

    let crate_name = flareon_ident();
    let result = quote! {
        fn main() {
            let body = async {
                let project: #crate_name::FlareonProject = __flareon_main().await.unwrap();
                #crate_name::run_cli(project).await.unwrap();

                #new_main_decl
            };
            #[allow(clippy::expect_used)]
            {
                return #crate_name::__private::tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed building the Runtime")
                    .block_on(body);
            }
        }

    };
    Ok(result)
}
