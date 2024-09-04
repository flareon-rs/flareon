use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Lookahead1, Parse, ParseStream};
use syn::spanned::Spanned;
use syn::Token;

#[derive(Debug)]
enum ItemToken {
    Field(FieldParser),
    Literal(syn::Lit),
    Ident(syn::Ident),
    MethodCall(MethodCallParser),
    Op(OpParser),
}

impl Parse for ItemToken {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        let op = OpParser::from_lookahead(&lookahead, input)?;
        if let Some(op) = op {
            return Ok(ItemToken::Op(op));
        }

        if lookahead.peek(Token![$]) {
            input.parse().map(ItemToken::Field)
        } else if lookahead.peek(Token![.]) {
            input.parse().map(ItemToken::MethodCall)
        } else if lookahead.peek(syn::Lit) {
            input.parse().map(ItemToken::Literal)
        } else if lookahead.peek(syn::Ident) {
            input.parse().map(ItemToken::Ident)
        } else {
            Err(lookahead.error())
        }
    }
}

impl ItemToken {
    fn span(&self) -> proc_macro2::Span {
        match self {
            ItemToken::Field(field) => field.span(),
            ItemToken::Literal(lit) => lit.span(),
            ItemToken::Ident(ident) => ident.span(),
            ItemToken::MethodCall(method_call) => method_call.span(),
            ItemToken::Op(op) => op.span(),
        }
    }
}

#[derive(Debug)]
struct FieldParser {
    _field_token: Token![$],
    name: syn::Ident,
}

impl FieldParser {
    #[must_use]
    fn span(&self) -> proc_macro2::Span {
        self.name.span()
    }
}

impl Parse for FieldParser {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(FieldParser {
            _field_token: input.parse()?,
            name: input.parse()?,
        })
    }
}

#[derive(Debug)]
struct MethodCallParser {
    _dot: Token![.],
    method_name: syn::Ident,
    _paren_token: syn::token::Paren,
    args: syn::punctuated::Punctuated<syn::Expr, Token![,]>,
}

impl MethodCallParser {
    #[must_use]
    fn span(&self) -> proc_macro2::Span {
        self.method_name.span()
    }
}

impl Parse for MethodCallParser {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let args_content;
        Ok(Self {
            _dot: input.parse()?,
            method_name: input.parse()?,
            _paren_token: syn::parenthesized!(args_content in input),
            args: args_content.parse_terminated(syn::Expr::parse, Token![,])?,
        })
    }
}

#[derive(Debug)]
enum OpParser {
    Mul(Token![*]),
    Div(Token![/]),
    Add(Token![+]),
    Sub(Token![-]),
    Eq(Token![==]),
    Ne(Token![!=]),
    And(Token![&&]),
    Or(Token![||]),
}

impl OpParser {
    fn from_lookahead(lookahead: &Lookahead1, input: ParseStream) -> syn::Result<Option<Self>> {
        let result = if lookahead.peek(Token![*]) {
            OpParser::Mul(input.parse()?)
        } else if lookahead.peek(Token![/]) {
            OpParser::Div(input.parse()?)
        } else if lookahead.peek(Token![+]) {
            OpParser::Add(input.parse()?)
        } else if lookahead.peek(Token![-]) {
            OpParser::Sub(input.parse()?)
        } else if lookahead.peek(Token![==]) {
            OpParser::Eq(input.parse()?)
        } else if lookahead.peek(Token![!=]) {
            OpParser::Ne(input.parse()?)
        } else if lookahead.peek(Token![&&]) {
            OpParser::And(input.parse()?)
        } else if lookahead.peek(Token![||]) {
            OpParser::Or(input.parse()?)
        } else {
            return Ok(None);
        };

        Ok(Some(result))
    }

    fn span(&self) -> proc_macro2::Span {
        match self {
            OpParser::Mul(mul) => mul.span(),
            OpParser::Div(div) => div.span(),
            OpParser::Add(add) => add.span(),
            OpParser::Sub(sub) => sub.span(),
            OpParser::Eq(eq) => eq.span(),
            OpParser::Ne(ne) => ne.span(),
            OpParser::And(and) => and.span(),
            OpParser::Or(or) => or.span(),
        }
    }

    fn infix_binding_priority(&self) -> InfixBindingPriority {
        match self {
            OpParser::Mul(_) | OpParser::Div(_) => InfixBindingPriority::left_to_right(9),
            OpParser::Add(_) | OpParser::Sub(_) => InfixBindingPriority::left_to_right(8),
            OpParser::Eq(_) | OpParser::Ne(_) => InfixBindingPriority::right_to_left(3),
            OpParser::And(_) => InfixBindingPriority::left_to_right(2),
            OpParser::Or(_) => InfixBindingPriority::left_to_right(1),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct BindingPriority<L, R> {
    left: L,
    right: R,
}

impl BindingPriority<u8, u8> {
    #[must_use]
    fn left_to_right(priority: u8) -> Self {
        debug_assert!(priority > 0);
        let right = priority * 2;
        BindingPriority {
            left: right - 1,
            right,
        }
    }

    #[must_use]
    fn right_to_left(priority: u8) -> Self {
        debug_assert!(priority > 0);
        let left = priority * 2;
        BindingPriority {
            left,
            right: left - 1,
        }
    }
}

type InfixBindingPriority = BindingPriority<u8, u8>;

#[derive(Debug, PartialEq, Eq)]
pub enum Expr {
    FieldRef(syn::Ident),
    Value(syn::Expr),
    MethodCall {
        called_on: Box<Expr>,
        method_name: syn::Ident,
        args: Vec<syn::Expr>,
    },
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn parse(input: TokenStream) -> syn::Result<Self> {
        syn::parse2::<Expr>(input)
    }

    fn parse_impl(input: ParseStream, min_binding_priority: u8) -> syn::Result<Self> {
        // Implementation of Pratt parsing algorithm

        let mut lhs = if input.peek(syn::token::Paren) {
            let content;
            let _ = syn::parenthesized!(content in input);
            Self::parse_impl(&content, 0)?
        } else {
            let lhs_item = input.parse::<ItemToken>()?;

            match lhs_item {
                ItemToken::Field(field) => Expr::FieldRef(field.name),
                ItemToken::Ident(ident) => Expr::Value(syn::Expr::Path(syn::ExprPath {
                    attrs: Vec::new(),
                    qself: None,
                    path: syn::Path::from(ident),
                })),
                ItemToken::Literal(lit) => Expr::Value(syn::Expr::Lit(syn::ExprLit {
                    attrs: Vec::new(),
                    lit,
                })),
                _ => {
                    return Err(syn::Error::new(
                        lhs_item.span(),
                        "expected field, literal, or identifier",
                    ))
                }
            }
        };

        loop {
            if input.is_empty() {
                break;
            }

            let op_item = input.fork().parse::<ItemToken>()?;
            match op_item {
                ItemToken::MethodCall(call) => {
                    input.parse::<ItemToken>()?;
                    let args = call.args.into_iter().collect::<Vec<_>>();
                    lhs = Expr::MethodCall {
                        called_on: Box::new(lhs),
                        method_name: call.method_name,
                        args,
                    };
                }
                ItemToken::Op(op) => {
                    let infix_binding_priority = op.infix_binding_priority();
                    if infix_binding_priority.left < min_binding_priority {
                        break;
                    }

                    input.parse::<ItemToken>()?;
                    let rhs = Self::parse_impl(input, infix_binding_priority.right)?;

                    lhs = Self::binary(lhs, &op, rhs);
                }
                _ => {
                    return Err(syn::Error::new(
                        op_item.span(),
                        "expected an operator or a method call",
                    ))
                }
            }
        }

        Ok(lhs)
    }

    #[must_use]
    fn binary(lhs: Expr, op: &OpParser, rhs: Expr) -> Self {
        match op {
            OpParser::Mul(_) => Expr::Mul(Box::new(lhs), Box::new(rhs)),
            OpParser::Div(_) => Expr::Div(Box::new(lhs), Box::new(rhs)),
            OpParser::Add(_) => Expr::Add(Box::new(lhs), Box::new(rhs)),
            OpParser::Sub(_) => Expr::Sub(Box::new(lhs), Box::new(rhs)),
            OpParser::Eq(_) => Expr::Eq(Box::new(lhs), Box::new(rhs)),
            OpParser::Ne(_) => Expr::Ne(Box::new(lhs), Box::new(rhs)),
            OpParser::And(_) => Expr::And(Box::new(lhs), Box::new(rhs)),
            OpParser::Or(_) => Expr::Or(Box::new(lhs), Box::new(rhs)),
        }
    }

    #[must_use]
    pub fn as_tokens(&self) -> Option<TokenStream> {
        match self {
            Expr::FieldRef(_) => None,
            Expr::Value(expr) => Some(quote! {#expr}),
            Expr::MethodCall {
                called_on,
                method_name,
                args,
            } => {
                let called_on_tokens = called_on.as_tokens()?;
                Some(quote! {#called_on_tokens.#method_name(#(#args),*)})
            }
            Expr::And(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens && #rhs_tokens})
            }
            Expr::Or(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens || #rhs_tokens})
            }
            Expr::Eq(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens == #rhs_tokens})
            }
            Expr::Ne(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens != #rhs_tokens})
            }
            Expr::Add(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens + #rhs_tokens})
            }
            Expr::Sub(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens - #rhs_tokens})
            }
            Expr::Mul(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens * #rhs_tokens})
            }
            Expr::Div(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens()?;
                let rhs_tokens = rhs.as_tokens()?;
                Some(quote! {#lhs_tokens / #rhs_tokens})
            }
        }
    }
}

impl Parse for Expr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Self::parse_impl(input, 0)
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::quote;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn test_field_ref() {
        let input = quote! { $field };
        let expected = Expr::FieldRef(syn::Ident::new("field", span()));

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_literal() {
        let input = quote! { 42 };
        let expected = Expr::Value(parse_quote!(42));

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_field_eq() {
        let input = quote! { $field == 42 };
        let expected = Expr::Eq(
            Box::new(Expr::FieldRef(syn::Ident::new("field", span()))),
            Box::new(Expr::Value(parse_quote!(42))),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_math() {
        let input = quote! { $a + $b * $c / $d - $e };
        let expected = Expr::Sub(
            Box::new(Expr::Add(
                Box::new(field("a")),
                Box::new(Expr::Div(
                    Box::new(Expr::Mul(Box::new(field("b")), Box::new(field("c")))),
                    Box::new(field("d")),
                )),
            )),
            Box::new(field("e")),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_field_eq_and() {
        let input = quote! { $field == 42 && $field != 42 };
        let expected = Expr::And(
            Box::new(Expr::Eq(
                Box::new(Expr::FieldRef(syn::Ident::new("field", span()))),
                Box::new(Expr::Value(parse_quote!(42))),
            )),
            Box::new(Expr::Ne(
                Box::new(Expr::FieldRef(syn::Ident::new("field", span()))),
                Box::new(Expr::Value(parse_quote!(42))),
            )),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_parenthesis_literal() {
        let input = quote! { (((($a)))) };
        let expected = field("a");

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_parenthesis_math() {
        let input = quote! { ($a + $b) * $c };
        let expected = Expr::Mul(
            Box::new(Expr::Add(Box::new(field("a")), Box::new(field("b")))),
            Box::new(field("c")),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_method_call() {
        let input = quote! { $a == foo.bar().baz() };
        let expected = Expr::Eq(
            Box::new(field("a")),
            Box::new(Expr::MethodCall {
                called_on: Box::new(Expr::MethodCall {
                    called_on: Box::new(value("foo")),
                    method_name: syn::Ident::new("bar", span()),
                    args: Vec::new(),
                }),
                method_name: syn::Ident::new("baz", span()),
                args: Vec::new(),
            }),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn test_tokens_field_ref() {
        let input = quote! { $migration.like("%this") };
        let expr = unwrap_syn(Expr::parse(input));

        assert!(expr.as_tokens().is_none());
    }

    #[test]
    fn test_tokens_method() {
        let input = quote! { string.contains("that") };
        let expr = unwrap_syn(Expr::parse(input));

        assert!(expr.as_tokens().is_some());
    }

    #[must_use]
    fn field(name: &str) -> Expr {
        Expr::FieldRef(syn::Ident::new(name, span()))
    }

    #[must_use]
    fn value(name: &str) -> Expr {
        let ident = syn::Ident::new(name, span());
        Expr::Value(parse_quote!(#ident))
    }

    /// Return an example span.
    #[must_use]
    fn span() -> Span {
        Span::call_site()
    }

    #[must_use]
    fn unwrap_syn<T>(result: syn::Result<T>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => {
                eprintln!("{err}");
                let pos = err.span().start();
                eprintln!("at line {} col {}", pos.line, pos.column + 1);
                if let Some(source_text) = err.span().source_text() {
                    eprintln!("{source_text}");
                }

                panic!("error occurred when parsing an expression");
            }
        }
    }
}
