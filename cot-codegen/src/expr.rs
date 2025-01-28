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
    MemberAccess(MemberAccessParser),
    FunctionCall(FunctionCallParser),
    Reference(ReferenceParser),
    Op(OpParser),
}

impl Parse for ItemToken {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        let op = OpParser::from_lookahead(&lookahead, input)?;
        if let Some(op) = op {
            return Ok(ItemToken::Op(op));
        }

        if lookahead.peek(Token![$]) {
            input.parse().map(ItemToken::Field)
        } else if lookahead.peek(Token![&]) {
            input.parse().map(ItemToken::Reference)
        } else if lookahead.peek(Token![.]) {
            input.parse().map(ItemToken::MemberAccess)
        } else if lookahead.peek(syn::token::Paren) {
            input.parse().map(ItemToken::FunctionCall)
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
            ItemToken::MemberAccess(member_access) => member_access.span(),
            ItemToken::FunctionCall(function_call) => function_call.span(),
            ItemToken::Reference(reference) => reference.span(),
            ItemToken::Op(op) => op.span(),
        }
    }
}

#[derive(Debug)]
struct FieldParser {
    field_token: Token![$],
    name: syn::Ident,
}

impl FieldParser {
    #[must_use]
    fn span(&self) -> proc_macro2::Span {
        self.name.span()
    }
}

impl Parse for FieldParser {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(FieldParser {
            field_token: input.parse()?,
            name: input.parse()?,
        })
    }
}

#[derive(Debug)]
struct ReferenceParser {
    reference_token: Token![&],
    expr: syn::Expr,
}

impl ReferenceParser {
    #[must_use]
    fn span(&self) -> proc_macro2::Span {
        self.expr.span()
    }
}

impl Parse for ReferenceParser {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(ReferenceParser {
            reference_token: input.parse()?,
            expr: input.parse()?,
        })
    }
}

#[derive(Debug)]
struct MemberAccessParser {
    dot: Token![.],
    member_name: syn::Ident,
}

impl MemberAccessParser {
    #[must_use]
    fn span(&self) -> proc_macro2::Span {
        self.member_name.span()
    }
}

impl Parse for MemberAccessParser {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(Self {
            dot: input.parse()?,
            member_name: input.parse()?,
        })
    }
}

#[derive(Debug)]
struct FunctionCallParser {
    args: syn::punctuated::Punctuated<syn::Expr, Token![,]>,
}

impl FunctionCallParser {
    #[must_use]
    fn span(&self) -> proc_macro2::Span {
        self.args.span()
    }
}

impl Parse for FunctionCallParser {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let args_content;
        syn::parenthesized!(args_content in input);
        Ok(Self {
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
    Lt(Token![<]),
    Lte(Token![<=]),
    Gt(Token![>]),
    Gte(Token![>=]),
    And(Token![&&]),
    Or(Token![||]),
}

impl OpParser {
    fn from_lookahead(
        lookahead: &Lookahead1<'_>,
        input: ParseStream<'_>,
    ) -> syn::Result<Option<Self>> {
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
        } else if lookahead.peek(Token![<=]) {
            OpParser::Lte(input.parse()?)
        } else if lookahead.peek(Token![<]) {
            OpParser::Lt(input.parse()?)
        } else if lookahead.peek(Token![>=]) {
            OpParser::Gte(input.parse()?)
        } else if lookahead.peek(Token![>]) {
            OpParser::Gt(input.parse()?)
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
            OpParser::Lt(lt) => lt.span(),
            OpParser::Lte(lte) => lte.span(),
            OpParser::Gt(gt) => gt.span(),
            OpParser::Gte(gte) => gte.span(),
            OpParser::And(and) => and.span(),
            OpParser::Or(or) => or.span(),
        }
    }

    fn infix_binding_priority(&self) -> InfixBindingPriority {
        match self {
            OpParser::Mul(_) | OpParser::Div(_) => InfixBindingPriority::left_to_right(9),
            OpParser::Add(_) | OpParser::Sub(_) => InfixBindingPriority::left_to_right(8),
            OpParser::Eq(_)
            | OpParser::Ne(_)
            | OpParser::Lt(_)
            | OpParser::Lte(_)
            | OpParser::Gt(_)
            | OpParser::Gte(_) => InfixBindingPriority::right_to_left(3),
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

/// A parsed expression.
///
/// This type represents a parsed expression that can be used to generate code.
///
/// # Examples
///
/// ```
/// use cot_codegen::expr::Expr;
/// use quote::quote;
/// use syn::parse_quote;
///
/// let expr = Expr::parse(quote! { $field == 42 }).unwrap();
/// assert_eq!(
///     expr,
///     Expr::Eq(
///         Box::new(Expr::FieldRef { field_name: parse_quote!(field), field_token: parse_quote!($)}),
///         Box::new(Expr::Value(parse_quote!(42)))
///     )
/// );
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum Expr {
    FieldRef {
        field_name: syn::Ident,
        field_token: Token![$],
    },
    Value(syn::Expr),
    MemberAccess {
        parent: Box<Expr>,
        member_name: syn::Ident,
        member_access_token: Token![.],
    },
    FunctionCall {
        function: Box<Expr>,
        args: Vec<syn::Expr>,
    },
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Lte(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Gte(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Parse an [`Expr`] from the given token stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid expression.
    pub fn parse(input: TokenStream) -> syn::Result<Self> {
        syn::parse2::<Expr>(input)
    }

    fn parse_impl(input: ParseStream<'_>, min_binding_priority: u8) -> syn::Result<Self> {
        // Implementation of Pratt parsing algorithm

        let mut lhs = if input.peek(syn::token::Paren) {
            let content;
            let _ = syn::parenthesized!(content in input);
            Self::parse_impl(&content, 0)?
        } else {
            let lhs_item = input.parse::<ItemToken>()?;

            match lhs_item {
                ItemToken::Field(field) => Expr::FieldRef {
                    field_name: field.name,
                    field_token: field.field_token,
                },
                ItemToken::Reference(reference) => {
                    Expr::Value(syn::Expr::Reference(syn::ExprReference {
                        attrs: Vec::new(),
                        and_token: reference.reference_token,
                        mutability: None,
                        expr: Box::new(reference.expr),
                    }))
                }
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
                ItemToken::MemberAccess(member_access) => {
                    input.parse::<ItemToken>()?;
                    lhs = Expr::MemberAccess {
                        parent: Box::new(lhs),
                        member_name: member_access.member_name,
                        member_access_token: member_access.dot,
                    };
                }
                ItemToken::FunctionCall(call) => {
                    input.parse::<ItemToken>()?;
                    let args = call.args.into_iter().collect::<Vec<_>>();
                    lhs = Expr::FunctionCall {
                        function: Box::new(lhs),
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
            OpParser::Lt(_) => Expr::Lt(Box::new(lhs), Box::new(rhs)),
            OpParser::Lte(_) => Expr::Lte(Box::new(lhs), Box::new(rhs)),
            OpParser::Gt(_) => Expr::Gt(Box::new(lhs), Box::new(rhs)),
            OpParser::Gte(_) => Expr::Gte(Box::new(lhs), Box::new(rhs)),
            OpParser::And(_) => Expr::And(Box::new(lhs), Box::new(rhs)),
            OpParser::Or(_) => Expr::Or(Box::new(lhs), Box::new(rhs)),
        }
    }

    #[must_use]
    pub fn as_tokens(&self) -> Option<TokenStream> {
        self.as_tokens_impl(ExprAsTokensMode::FieldRefAsNone)
    }

    #[must_use]
    pub fn as_tokens_full(&self) -> TokenStream {
        self.as_tokens_impl(ExprAsTokensMode::Full)
            .expect("Full mode should never return None")
    }

    #[must_use]
    fn as_tokens_impl(&self, mode: ExprAsTokensMode) -> Option<TokenStream> {
        match self {
            Expr::FieldRef {
                field_name,
                field_token,
            } => match mode {
                ExprAsTokensMode::FieldRefAsNone => None,
                ExprAsTokensMode::Full => Some(quote! {#field_token #field_name}),
            },
            Expr::Value(expr) => Some(quote! {#expr}),
            Expr::MemberAccess {
                parent,
                member_name,
                member_access_token,
            } => {
                let parent_tokens = parent.as_tokens_impl(mode)?;
                Some(quote! {#parent_tokens #member_access_token #member_name})
            }
            Expr::FunctionCall { function, args } => {
                let function_tokens = function.as_tokens_impl(mode)?;
                Some(quote! {#function_tokens(#(#args),*)})
            }
            Expr::And(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens && #rhs_tokens})
            }
            Expr::Or(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens || #rhs_tokens})
            }
            Expr::Eq(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens == #rhs_tokens})
            }
            Expr::Ne(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens != #rhs_tokens})
            }
            Expr::Lt(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens < #rhs_tokens})
            }
            Expr::Lte(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens <= #rhs_tokens})
            }
            Expr::Gt(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens > #rhs_tokens})
            }
            Expr::Gte(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens >= #rhs_tokens})
            }
            Expr::Add(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens + #rhs_tokens})
            }
            Expr::Sub(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens - #rhs_tokens})
            }
            Expr::Mul(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens * #rhs_tokens})
            }
            Expr::Div(lhs, rhs) => {
                let lhs_tokens = lhs.as_tokens_impl(mode)?;
                let rhs_tokens = rhs.as_tokens_impl(mode)?;
                Some(quote! {#lhs_tokens / #rhs_tokens})
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ExprAsTokensMode {
    FieldRefAsNone,
    Full,
}

impl Parse for Expr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
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
    fn field_ref() {
        let input = quote! { $field };
        let expected = field("field");

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn literal() {
        let input = quote! { 42 };
        let expected = Expr::Value(parse_quote!(42));

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn field_eq() {
        let input = quote! { $field == 42 };
        let expected = Expr::Eq(
            Box::new(field("field")),
            Box::new(Expr::Value(parse_quote!(42))),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn math() {
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
    fn field_eq_and() {
        let input = quote! { $field == 42 && $field != 42 };
        let expected = Expr::And(
            Box::new(Expr::Eq(
                Box::new(field("field")),
                Box::new(Expr::Value(parse_quote!(42))),
            )),
            Box::new(Expr::Ne(
                Box::new(field("field")),
                Box::new(Expr::Value(parse_quote!(42))),
            )),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn parenthesis_literal() {
        let input = quote! { (((($a)))) };
        let expected = field("a");

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn parenthesis_math() {
        let input = quote! { ($a + $b) * $c };
        let expected = Expr::Mul(
            Box::new(Expr::Add(Box::new(field("a")), Box::new(field("b")))),
            Box::new(field("c")),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn function_call() {
        let input = quote! { $a == bar() };
        let expected = Expr::Eq(
            Box::new(field("a")),
            Box::new(Expr::FunctionCall {
                function: Box::new(value("bar")),
                args: Vec::new(),
            }),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn function_call_with_args() {
        let input = quote! { $a == bar(42, "baz") };
        let expected = Expr::Eq(
            Box::new(field("a")),
            Box::new(Expr::FunctionCall {
                function: Box::new(value("bar")),
                args: vec![parse_quote!(42), parse_quote!("baz")],
            }),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn parse_member_access() {
        let input = quote! { $a == foo.bar };
        let expected = Expr::Eq(
            Box::new(field("a")),
            Box::new(member_access(value("foo"), "bar")),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn parse_member_access_multiple() {
        let input = quote! { $a == foo.bar.baz };
        let expected = Expr::Eq(
            Box::new(field("a")),
            Box::new(member_access(member_access(value("foo"), "bar"), "baz")),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn parse_reference() {
        let input = quote! { &foo };
        let expected = reference("foo");

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn method_call() {
        let input = quote! { $a == foo.bar().baz() };
        let expected = Expr::Eq(
            Box::new(field("a")),
            Box::new(Expr::FunctionCall {
                function: Box::new(member_access(
                    Expr::FunctionCall {
                        function: Box::new(member_access(value("foo"), "bar")),
                        args: Vec::new(),
                    },
                    "baz",
                )),
                args: Vec::new(),
            }),
        );

        assert_eq!(expected, unwrap_syn(Expr::parse(input)));
    }

    #[test]
    fn tokens_field_ref() {
        let input = quote! { $migration.like("%this") };
        let expr = unwrap_syn(Expr::parse(input));

        assert!(expr.as_tokens().is_none());
    }

    #[test]
    fn tokens_method() {
        let input = quote! { string.contains("that") };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_eq() {
        let input = quote! { x == 42 };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_ne() {
        let input = quote! { x != 42 };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_lt() {
        let input = quote! { x < 42 };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_lte() {
        let input = quote! { x <= 42 };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_gt() {
        let input = quote! { x > 42 };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_gte() {
        let input = quote! { x >= 42 };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_and() {
        let input = quote! { x && y };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_or() {
        let input = quote! { x || y };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_add() {
        let input = quote! { x + y };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_sub() {
        let input = quote! { x - y };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_mul() {
        let input = quote! { x * y };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_div() {
        let input = quote! { x / y };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens().unwrap().to_string());
    }

    #[test]
    fn tokens_full() {
        let input = quote! { $name.len() };
        let expr = unwrap_syn(Expr::parse(input.clone()));

        assert_eq!(input.to_string(), expr.as_tokens_full().to_string());
    }

    #[must_use]
    fn field(name: &str) -> Expr {
        Expr::FieldRef {
            field_name: syn::Ident::new(name, span()),
            field_token: Token![$](span()),
        }
    }

    #[must_use]
    fn member_access(parent: Expr, member_name: &str) -> Expr {
        Expr::MemberAccess {
            parent: Box::new(parent),
            member_name: syn::Ident::new(member_name, span()),
            member_access_token: Token![.](span()),
        }
    }

    #[must_use]
    fn reference(ident: &str) -> Expr {
        let ident = syn::Ident::new(ident, span());
        Expr::Value(parse_quote!(&#ident))
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
