//! Statements, i.e. the contents of a `{ ... }` block.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Braced,
        expression::{Expression, ExpressionWithBlock, ExpressionWithoutBlock},
        item::Item,
        pattern::Pattern,
        r#type::Type,
        tokens::{Colon, Else, Eq, Let, Semicolon},
    },
    error::Diagnostics,
    parse::Parse,
};

/// One statement inside a block: an empty `;`, a local item declaration, or
/// an expression.
///
/// Both expression variants carry an optional trailing `;` — a
/// semicolon-less expression (with or without a block) in tail position is
/// the block's value.
///
/// Reference: <https://doc.rust-lang.org/reference/statements.html>
#[derive(Clone, Debug)]
pub enum Statement {
    /// An empty `;` statement.
    Semicolon(Semicolon),
    /// A local item declaration.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/statements.html#item-declarations>
    Item(Box<Item>),
    /// A `let` statement: `let PATTERN: Type? = EXPR (else { ... })?;`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/statements.html#let-statements>
    Let(LetStatement),
    /// An expression ending in a block, with an optional trailing `;`
    /// (needed, unlike a bare `;`-terminated statement, so a `for`/`while`/
    /// `match`/`if`/bare-block expression can end a block in tail
    /// position, with no `;`, as its value).
    ///
    /// Reference: <https://doc.rust-lang.org/reference/statements.html#expression-statements>
    ExpressionWithBlock(ExpressionWithBlock, Option<Semicolon>),
    /// An expression without a block, with an optional trailing `;`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/statements.html#expression-statements>
    ExpressionWithoutBlock(ExpressionWithoutBlock, Option<Semicolon>),
}

/// A `let` statement: `let PATTERN: Type? = EXPR (else { ... })?;`.
///
/// The `else` block (let-else) requires `expr` to not itself end in a
/// block, to avoid ambiguity with the `else` — not enforced here (accepted
/// more permissively than rustc).
///
/// Reference: <https://doc.rust-lang.org/reference/statements.html#let-statements>
#[derive(Clone, Debug)]
pub struct LetStatement {
    let_token: Let,
    pat: Pattern,
    ty: Option<(Colon, Type)>,
    eq: Eq,
    expr: Expression,
    else_branch: Option<(Else, Braced<Vec<Statement>>)>,
    semicolon: Semicolon,
}

impl Parse for Statement {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(semicolon) = input.try_parse() {
            Ok(Self::Semicolon(semicolon))
        } else if let Ok(item) = input.try_parse() {
            Ok(Self::Item(item))
        } else if let Ok(let_statement) = input.try_parse() {
            Ok(Self::Let(let_statement))
        } else if let Ok((expression, semicolon)) = input.try_parse() {
            Ok(Self::ExpressionWithBlock(expression, semicolon))
        } else if let Ok((expression, semicolon)) = input.try_parse() {
            Ok(Self::ExpressionWithoutBlock(expression, semicolon))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected a statement",
                input.span(),
            ))
        }
    }
}

impl Parse for LetStatement {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            let_token: input.parse()?,
            pat: input.parse()?,
            ty: input.try_parse().ok(),
            eq: input.parse()?,
            expr: input.parse()?,
            else_branch: if let Ok(else_token) = input.try_parse() {
                Some((else_token, input.parse()?))
            } else {
                None
            },
            semicolon: input.parse()?,
        })
    }
}

impl ToTokens for Statement {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Semicolon(semicolon) => semicolon.to_tokens(tokens),
            Self::Item(item) => item.to_tokens(tokens),
            Self::Let(let_statement) => let_statement.to_tokens(tokens),
            Self::ExpressionWithBlock(expression, semicolon) => {
                expression.to_tokens(tokens);
                semicolon.to_tokens(tokens);
            }
            Self::ExpressionWithoutBlock(expression, semicolon) => {
                expression.to_tokens(tokens);
                semicolon.to_tokens(tokens);
            }
        }
    }
}

impl ToTokens for LetStatement {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.let_token.to_tokens(tokens);
        self.pat.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.eq.to_tokens(tokens);
        self.expr.to_tokens(tokens);
        self.else_branch.to_tokens(tokens);
        self.semicolon.to_tokens(tokens);
    }
}
