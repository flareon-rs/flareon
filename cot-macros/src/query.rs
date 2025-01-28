use cot_codegen::expr::Expr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::Token;

use crate::cot_ident;

#[derive(Debug)]
pub(crate) struct Query {
    model_name: syn::Type,
    _comma: Token![,],
    expr: Expr,
}

impl Parse for Query {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(Self {
            model_name: input.parse()?,
            _comma: input.parse()?,
            expr: input.parse()?,
        })
    }
}

pub(super) fn query_to_tokens(query: Query) -> TokenStream {
    let crate_name = cot_ident();
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

    let crate_name = cot_ident();
    match expr {
        Expr::FieldRef { field_name, .. } => {
            quote!(<#model_name as #crate_name::db::Model>::Fields::#field_name.as_expr())
        }
        Expr::Value(value) => {
            quote!(#crate_name::db::query::Expr::value(#value))
        }
        Expr::MemberAccess {
            parent,
            member_name,
            ..
        } => match parent.as_tokens() {
            Some(tokens) => {
                quote!(#crate_name::db::query::Expr::value(#tokens.#member_name))
            }
            None => syn::Error::new_spanned(
                parent.as_tokens_full(),
                "accessing members of values that reference database fields is unsupported",
            )
            .to_compile_error(),
        },
        Expr::FunctionCall { function, args } => match function.as_tokens() {
            Some(tokens) => {
                quote!(#crate_name::db::query::Expr::value(#tokens(#(#args),*)))
            }
            None => syn::Error::new_spanned(
                function.as_tokens_full(),
                "calling functions that reference database fields is unsupported",
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
        Expr::Lt(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "lt", "ExprOrd"),
        Expr::Lte(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "lte", "ExprOrd"),
        Expr::Gt(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "gt", "ExprOrd"),
        Expr::Gte(lhs, rhs) => handle_binary_comparison(model_name, *lhs, *rhs, "gte", "ExprOrd"),
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
    let crate_name = cot_ident();
    let bin_fn = format_ident!("{}", bin_fn);
    let bin_trait = format_ident!("{}", bin_trait);

    if let Expr::FieldRef { ref field_name, .. } = lhs {
        if let Some(rhs_tokens) = rhs.as_tokens() {
            return quote!(#crate_name::db::query::#bin_trait::#bin_fn(<#model_name as #crate_name::db::Model>::Fields::#field_name, #rhs_tokens));
        }
    }

    let lhs = expr_to_tokens(model_name, lhs);
    let rhs = expr_to_tokens(model_name, rhs);
    quote!(#crate_name::db::query::Expr::#bin_fn(#lhs, #rhs))
}
