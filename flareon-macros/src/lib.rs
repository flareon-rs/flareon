use proc_macro::TokenStream;
use syn::token::Token;
use syn::Token;

#[proc_macro_attribute]
pub fn model(attr: TokenStream, item: TokenStream) -> TokenStream {
    println!("ATTRIBUTE");
    println!("Attr: {attr}");
    println!("Item: {item}");

    let ast: syn::DeriveInput = syn::parse(item).unwrap();
    println!("Ident: {:?}", &ast.ident);

    let syn::Data::Struct(datastruct) = ast.data else {
        unimplemented!()
    };

    let syn::Fields::Named(namedfields) = datastruct.fields else {
        unimplemented!()
    };

    println!("Fields:");
    for field in namedfields.named.iter() {
        println!("\tid: {:?}", field.ident);
    }

    TokenStream::new()
}
