//! Statements, i.e. the contents of a `{ ... }` block.

use crate::ToTokens;

use crate::{
    ast::{
        attributes::{Attribute, parse_inner_attributes, parse_outer_attributes},
        delimiter::Braced,
        expression::{
            Expression, ExpressionWithBlock, ExpressionWithoutBlock, continues_with_postfix,
        },
        item::Item,
        path::SimplePath,
        pattern::Pattern,
        tokens::{Colon, Else, Eq, Let, Not, Semicolon},
        r#type::Type,
    },
    error::Diagnostics,
    parse::Parse,
    proc_macro::Delimiter,
};

/// The contents of a `{ ... }` block: inner attributes (`#![...]`), then
/// statements.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html>
#[derive(Clone, Default)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct Block {
    inner_attributes: Vec<Attribute>,
    statements: Vec<Statement>,
}

impl Block {
    /// The block's inner attributes, e.g. `#![allow(unused)]`.
    #[must_use]
    pub fn inner_attributes(&self) -> &[Attribute] {
        &self.inner_attributes
    }
    /// The block's statements, in order.
    #[must_use]
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }
    /// Mutable access to the block's statements.
    pub const fn statements_mut(&mut self) -> &mut Vec<Statement> {
        &mut self.statements
    }
}

impl Parse for Block {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let inner_attributes = parse_inner_attributes(input);
        let mut statements = Vec::new();
        while !input.is_empty() {
            let statement: Statement = input.parse()?;
            // Only the block's tail may omit its `;`.
            if !input.is_empty() && statement.needs_semicolon() {
                return Err(Diagnostics::new_error_spanned("Expected `;`", input.span()));
            }
            statements.push(statement);
        }
        Ok(Self {
            inner_attributes,
            statements,
        })
    }
}

impl ToTokens for Block {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.inner_attributes.to_tokens(tokens);
        self.statements.to_tokens(tokens);
    }
}

/// One statement inside a block: an empty `;`, a local item declaration, or
/// an expression.
///
/// Both expression variants carry an optional trailing `;` — a
/// semicolon-less expression (with or without a block) in tail position is
/// the block's value.
///
/// Reference: <https://doc.rust-lang.org/reference/statements.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
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

/// A `let` statement, with its outer attributes:
/// `let PATTERN (: Type)? (= EXPR (else { ... })?)?;`.
///
/// The `else` block (let-else) requires `expr` to not itself end in a
/// block, to avoid ambiguity with the `else` — not enforced here (accepted
/// more permissively than rustc).
///
/// Reference: <https://doc.rust-lang.org/reference/statements.html#let-statements>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct LetStatement {
    attributes: Vec<Attribute>,
    let_token: Let,
    pat: Pattern,
    ty: Option<(Colon, Type)>,
    init: Option<LetInit>,
    semicolon: Semicolon,
}

/// The `= EXPR (else { ... })?` initializer of a [`LetStatement`].
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
struct LetInit {
    eq: Eq,
    expr: Expression,
    else_branch: Option<(Else, Braced<Block>)>,
}

/// Whether the macro invocation at `input` (if any) is the start of an
/// expression statement rather than a macro statement, following `rustc`:
/// a braced invocation is a statement unless a postfix `.`/`?` follows
/// (`m! {}.len()`), and any other invocation is a statement only when
/// followed by `;` or the end of the block (`m!().len()`, `m![] + 1` are
/// expressions).
fn is_expression_macro(input: &crate::parse::ParseBuffer) -> bool {
    let mut fork = input.clone();
    parse_outer_attributes(&mut fork);
    if fork.parse::<SimplePath>().is_err() || fork.parse::<Not>().is_err() {
        return false;
    }
    let Some(group) = fork.group() else {
        return false;
    };
    if group.delimiter() == Delimiter::Brace {
        continues_with_postfix(&fork)
    } else {
        !(fork.is_empty() || fork.peek_punct_char().is_some_and(|(ch, _)| ch == ';'))
    }
}

impl Statement {
    /// Whether this statement must be followed by a `;` unless it ends its
    /// block: an expression without a block, or a `(...)`/`[...]` macro
    /// invocation, with no `;`.
    fn needs_semicolon(&self) -> bool {
        match self {
            Self::ExpressionWithoutBlock(_, semicolon) => semicolon.is_none(),
            Self::Item(item) => match &**item {
                Item::MacroInvocation(invocation) => invocation.needs_semicolon(),
                _ => false,
            },
            Self::Semicolon(_) | Self::Let(_) | Self::ExpressionWithBlock(..) => false,
        }
    }
}

impl Parse for Statement {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(semicolon) = input.try_parse() {
            Ok(Self::Semicolon(semicolon))
        } else if !is_expression_macro(input)
            && let Ok(item) = input.try_parse()
        {
            Ok(Self::Item(item))
        } else if let Ok(let_statement) = input.try_parse() {
            Ok(Self::Let(let_statement))
        } else if let Ok((expression, semicolon)) = input.try_advance(|input| {
            // a block-like expression ends the statement at its `}` (so
            // `{ a } - 1` is two statements), unless a postfix `.`/`?`
            // follows: `match x { .. }.len()` falls through to the
            // without-block branch below.
            let expression: ExpressionWithBlock = input.parse()?;
            if continues_with_postfix(input) {
                return Err(Diagnostics::new_error_spanned(
                    "Expected the end of a statement",
                    input.span(),
                ));
            }
            Ok((expression, input.try_parse().ok()))
        }) {
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
        let attributes = parse_outer_attributes(input);
        let let_token = input.parse()?;
        let pat = input.parse()?;
        let ty = input.try_parse().ok();
        let init = if let Ok(eq) = input.try_parse() {
            Some(LetInit {
                eq,
                expr: input.parse()?,
                else_branch: if let Ok(else_token) = input.try_parse() {
                    Some((else_token, input.parse()?))
                } else {
                    None
                },
            })
        } else {
            None
        };
        Ok(Self {
            attributes,
            let_token,
            pat,
            ty,
            init,
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
        self.attributes.to_tokens(tokens);
        self.let_token.to_tokens(tokens);
        self.pat.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        if let Some(init) = &self.init {
            init.eq.to_tokens(tokens);
            init.expr.to_tokens(tokens);
            init.else_branch.to_tokens(tokens);
        }
        self.semicolon.to_tokens(tokens);
    }
}
