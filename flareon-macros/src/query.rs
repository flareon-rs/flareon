use flareon_codegen::expr::Expr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::Token;

use crate::flareon_ident;

#[derive(Debug)]
pub struct Query {
    model_name: syn::Type,
    _comma: Token![,],
    expr: Expr,
}

impl Parse for Query {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            model_name: input.parse()?,
            _comma: input.parse()?,
            expr: input.parse()?,
        })
    }
}

pub(super) fn query_to_tokens(query: Query) -> TokenStream {
    let crate_name = flareon_ident();
    let model_name = query.model_name;
    let expr = expr_to_tokens(&model_name, query.expr);

    quote! {
        <#model_name as #crate_name::db::Model>::objects().filter(#expr)
    }
}

pub(super) fn expr_to_tokens(model_name: &syn::Type, expr: Expr) -> TokenStream {
    if let Some(tokens) = expr.as_tokens() {
        return tokens;
    }

    let crate_name = flareon_ident();
    match expr {
        Expr::FieldRef(name) => {
            quote!(<#model_name as #crate_name::db::Model>::Fields::#name.as_expr())
        }
        Expr::Value(value) => {
            quote!(#crate_name::db::query::Expr::value(#value))
        }
        Expr::MethodCall {
            called_on,
            method_name,
            args,
        } => match *called_on {
            Expr::Value(syn_expr) => {
                quote!(#crate_name::db::query::Expr::value(#syn_expr.#method_name(#(#args),*)))
            }
            _ => syn::Error::new(
                method_name.span(),
                "only method calls on values are supported",
            )
            .to_compile_error(),
        },
        Expr::And(lhs, rhs) => {
            let lhs = expr_to_tokens(model_name, *lhs);
            let rhs = expr_to_tokens(model_name, *rhs);
            quote!(#crate_name::db::query::Expr::and(#lhs, #rhs))
        }
        Expr::Or(lhs, rhs) => {
            let lhs = expr_to_tokens(model_name, *lhs);
            let rhs = expr_to_tokens(model_name, *rhs);
            quote!(#crate_name::db::query::Expr::or(#lhs, #rhs))
        }
        Expr::Eq(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "eq", "ExprEq"),
        Expr::Ne(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "ne", "ExprEq"),
        Expr::Add(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "add", "ExprAdd"),
        Expr::Sub(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "sub", "ExprSub"),
        Expr::Mul(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "mul", "ExprMul"),
        Expr::Div(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "div", "ExprDiv"),
    }
}

fn handle_binary_comparison(
    model_name: &syn::Type,
    lhs: Expr,
    rhs: Expr,
    bin_fn: &str,
    bin_trait: &str,
) -> TokenStream {
    let crate_name = flareon_ident();
    let bin_fn = format_ident!("{}", bin_fn);
    let bin_trait = format_ident!("{}", bin_trait);

    if let Expr::FieldRef(ref field) = lhs {
        if let Some(rhs_tokens) = rhs.as_tokens() {
            return quote!(#crate_name::db::query::#bin_trait::#bin_fn(<#model_name as #crate_name::db::Model>::Fields::#field, #rhs_tokens));
        }
    }

    let lhs = expr_to_tokens(model_name, lhs);
    let rhs = expr_to_tokens(model_name, rhs);
    quote!(#crate_name::db::query::Expr::#bin_fn(#lhs, #rhs))
}
