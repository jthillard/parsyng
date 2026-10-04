//! Paths ([`SimplePath`]) and the generics/Fn-trait-sugar grammar attached to
//! a type path segment ([`TypePathSegment`], [`GenericArgs`]).

use crate::ToTokens;
use crate::ast::delimiter::Parenthesized;
use crate::ast::tokens::{Eq, Minus, RArrow};

use crate::combinator::Either;
use crate::proc_macro::Delimiter;
use crate::{
    ast::{
        item::{Lifetime, TypeParamBounds},
        tokens::{Colon, Comma, Gt, Lt, PathSep},
        r#type::Type,
    },
    combinator::{Punctuated, StopOnError},
    error::{Diagnostics, Result},
    parse::{Parse, ParseBuffer, Peekable},
    proc_macro::{Group, Ident, Literal, Span},
};

/// A path with no generic arguments, e.g. `std::mem::swap` or
/// `::foo::bar`. Used where generics don't make sense syntactically — `use`
/// trees, macro invocation paths, `pub(in path::to::mod)`.
///
/// For a path that may carry generic arguments on its segments (`Vec<T>`,
/// `<T as Trait>::Assoc`), see
/// [`TypePath`](crate::ast::type::TypePath) instead.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#simple-paths>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct SimplePath {
    start_token: Option<PathSep>,
    root: Ident,
    paths: Vec<(PathSep, Ident)>,
}

impl Parse for SimplePath {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            start_token: parse_leading_path_sep(input),
            root: input.parse()?,
            paths: parse_path_tail(input, Ident::parse),
        })
    }
}

/// Whether the next two tokens are `::`.
pub(crate) fn is_path_sep(input: &ParseBuffer) -> bool {
    input.nth_punct_char(0) == Some((':', true))
        && input.nth_punct_char(1).is_some_and(|(ch, _)| ch == ':')
}

/// An optional leading `::`.
pub(crate) fn parse_leading_path_sep(input: &mut ParseBuffer) -> Option<PathSep> {
    if is_path_sep(input) {
        input.parse().ok()
    } else {
        None
    }
}

/// The `::segment` tail of a path: like `Vec<(PathSep, S)>`, but only
/// attempts another segment when a `::` follows.
pub(crate) fn parse_path_tail<S>(
    input: &mut ParseBuffer,
    segment: impl Fn(&mut ParseBuffer) -> crate::error::Result<S>,
) -> Vec<(PathSep, S)> {
    let mut paths = Vec::new();
    while is_path_sep(input) {
        match input.try_advance(|input| Ok((input.parse::<PathSep>()?, segment(input)?))) {
            Ok(pair) => paths.push(pair),
            Err(_) => break,
        }
    }
    paths
}

impl SimplePath {
    /// The span of this path's first token (its leading `::`, if any).
    #[must_use]
    pub fn span(&self) -> Span {
        self.start_token
            .as_ref()
            .map_or_else(|| self.root.span(), |start| start.spans()[0])
    }
    /// This path's sole identifier, if it has no leading `::` and no
    /// additional `::`-separated segments.
    ///
    /// Used by [`ast::pattern`](crate::ast::pattern) to tell a bare
    /// identifier pattern (`name`) apart from a longer path pattern
    /// (`Foo::Bar`).
    #[must_use]
    #[cfg(feature = "full")]
    pub(crate) const fn as_single_ident(&self) -> Option<&Ident> {
        if self.start_token.is_none() && self.paths.is_empty() {
            Some(&self.root)
        } else {
            None
        }
    }
}

impl ToTokens for SimplePath {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.start_token.to_tokens(tokens);
        self.root.to_tokens(tokens);
        self.paths.to_tokens(tokens);
    }
}

/// One segment of a [`TypePath`](crate::ast::type::TypePath), e.g. `Vec`
/// in `Vec<T>`, or `FnOnce` in `FnOnce(A) -> B`.
///
/// `args` covers both the angle-bracket form (`<T>`, via [`GenericArgs`]) and
/// the Fn-trait-sugar form (`(A) -> B`, via [`TypePathFn`]).
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypePathSegment {
    path_ident: Ident,
    args: Option<(Option<PathSep>, Either<GenericArgs, TypePathFn>)>,
}

impl TypePathSegment {
    /// The span of this segment's identifier.
    #[must_use]
    pub fn span(&self) -> Span {
        self.path_ident.span()
    }

    /// Parse a segment of a path in expression position, where generic
    /// arguments require the turbofish (`f::<T>`) and the `Fn(A) -> B`
    /// sugar doesn't apply — so `a < b` stays a comparison.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
    #[cfg(feature = "full")]
    pub(crate) fn parse_expression(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            path_ident: input.parse()?,
            args: input
                .try_parse::<(PathSep, GenericArgs)>()
                .ok()
                .map(|(sep, generics)| (Some(sep), Either::First(generics))),
        })
    }
}

/// The `Fn`-trait sugar form of a path segment's arguments: `(A, B) -> C`.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypePathFn {
    // Boxed: rare, and it would otherwise make every path segment much larger.
    inputs: Parenthesized<Option<Box<TypePathFnInputs>>>,
    return_type: Option<(RArrow, Box<Type>)>,
}

/// The comma-separated argument types inside [`TypePathFn`]'s parentheses.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypePathFnInputs {
    args: Punctuated<Type, Comma>,
}

/// Angle-bracketed generic arguments on a path segment: `<A, 'a, B = C>`.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct GenericArgs {
    start_token: Lt,
    generics: Punctuated<GenericArg, Comma, StopOnError>,
    last_token: Gt,
}

/// One argument inside [`GenericArgs`]: a type, a lifetime, an associated
/// type binding, or a const argument.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum GenericArg {
    /// A type argument.
    Type(Box<Type>),
    /// A lifetime argument.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#lifetimes-and-loop-labels>
    Lifetime(Lifetime),
    /// An associated-type binding, e.g. `Item = T` in `Iterator<Item = T>`
    /// (the optional nested [`GenericArgs`] covers a binding on a generic
    /// associated type, e.g. `Item<'a> = T`).
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-types>
    Bindings(Ident, Option<Box<GenericArgs>>, Eq, Box<Type>),
    /// An associated-type bound, e.g. `Item: Clone` in
    /// `Iterator<Item: Clone>` (the optional nested [`GenericArgs`] covers a
    /// generic associated type, e.g. `Item<'a>: Clone`).
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
    Constraint(Ident, Option<Box<GenericArgs>>, Colon, TypeParamBounds),
    /// A const argument: a `{ ... }` block, a literal, or a negated
    /// literal, e.g. `{ N + 1 }` in `f::<{ N + 1 }>()`. A bare `N` parses
    /// as a [`Type`](Self::Type) argument, since the two can't be told
    /// apart syntactically.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
    Const(ConstArg),
}

/// A const generic argument, or a const generic parameter's default: a
/// `{ ... }` block (kept as raw tokens), a literal, a negated literal, or an
/// identifier.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html#const-generics>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum ConstArg {
    /// A `{ ... }` block.
    Block(Group),
    /// A literal, e.g. `3`.
    Literal(Literal),
    /// A negated literal, e.g. `-3`.
    Negated(Minus, Literal),
    /// A const generic parameter or constant, e.g. `N`.
    Ident(Ident),
}

impl Parse for ConstArg {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if input.peek_delimiter() == Some(Delimiter::Brace) {
            return Ok(Self::Block(input.parse()?));
        }
        if input.peek_literal().is_some() {
            return Ok(Self::Literal(input.parse()?));
        }
        if input.peek_punct_char().is_some_and(|(ch, _)| ch == '-') {
            return input.try_advance(|input| Ok(Self::Negated(input.parse()?, input.parse()?)));
        }
        if input.peek_ident_str().is_some() {
            return Ok(Self::Ident(input.parse()?));
        }
        Err(Diagnostics::new_error_spanned(
            "Expected a const generic argument",
            input.span(),
        ))
    }
}

impl ToTokens for ConstArg {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Block(group) => group.to_tokens(tokens),
            Self::Literal(literal) => literal.to_tokens(tokens),
            Self::Negated(minus, literal) => (minus, literal).to_tokens(tokens),
            Self::Ident(ident) => ident.to_tokens(tokens),
        }
    }
}

impl ToTokens for TypePathSegment {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path_ident.to_tokens(tokens);
        self.args.to_tokens(tokens);
    }
}

impl Parse for TypePathSegment {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let path_ident = input.parse()?;
        // Generic arguments start with `<`/`(`, optionally after `::`: only
        // attempt them when one is there (most segments have none).
        let offset = if input.nth_punct_char(0) == Some((':', true))
            && input.nth_punct_char(1).is_some_and(|(ch, _)| ch == ':')
        {
            2
        } else {
            0
        };
        let has_args = input
            .nth_punct_char(offset)
            .is_some_and(|(ch, _)| ch == '<')
            || input.nth_delimiter(offset) == Some(Delimiter::Parenthesis);
        let args = if has_args {
            input
                .try_advance(|input| {
                    let sep = if offset == 2 {
                        Some(input.parse()?)
                    } else {
                        None
                    };
                    Ok((sep, input.parse()?))
                })
                .ok()
        } else {
            None
        };
        Ok(Self { path_ident, args })
    }
}
impl ToTokens for GenericArg {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Type(ty) => ty.to_tokens(tokens),
            Self::Lifetime(lifetime) => lifetime.to_tokens(tokens),
            Self::Bindings(ident, generics, eq, ty) => (ident, generics, eq, ty).to_tokens(tokens),
            Self::Constraint(ident, generics, colon, bounds) => {
                (ident, generics, colon, bounds).to_tokens(tokens);
            }
            Self::Const(arg) => arg.to_tokens(tokens),
        }
    }
}

/// What follows a `Name` at the cursor in generic arguments.
#[derive(PartialEq, Eq)]
enum AfterName {
    /// `=`: an associated-type binding (`Item = T`).
    Eq,
    /// `:` (not `::`): an associated-type bound (`Item: Clone`).
    Colon,
    /// Anything else: a type or a const argument.
    Other,
}

/// Classify the `=`/`:` (if any) at the cursor.
fn after_name(input: &ParseBuffer, n: u32) -> AfterName {
    match input.nth_punct_char(n) {
        // Not `==`.
        Some(('=', false)) => AfterName::Eq,
        Some(('=', true)) if input.nth_punct_char(n + 1).is_none_or(|(ch, _)| ch != '=') => {
            AfterName::Eq
        }
        // Not `::`.
        Some((':', false)) => AfterName::Colon,
        Some((':', true)) if input.nth_punct_char(n + 1).is_none_or(|(ch, _)| ch != ':') => {
            AfterName::Colon
        }
        _ => AfterName::Other,
    }
}

/// Whether the `Name<..>` at the cursor is followed by `=` or `:`, making
/// it an associated-type binding (`Item<'a> = T`) or bound (`Item<'a>:
/// Clone`) rather than a type: a token scan, so that `Option<Box<..>>`
/// isn't parsed once as a failed binding then again as a type at every
/// nesting level.
fn generic_name_followed_by(input: &ParseBuffer) -> AfterName {
    let mut cursor = input.clone();
    cursor.bump_token();
    let mut depth = 0u32;
    // Whether the previous token was a joint `-` (the `>` of `->` doesn't
    // close anything).
    let mut after_minus = false;
    loop {
        let punct = cursor.peek_punct_char();
        if !cursor.bump_token() {
            return AfterName::Other;
        }
        match punct {
            Some(('<', _)) => depth += 1,
            Some(('>', _)) if !after_minus => {
                depth -= 1;
                if depth == 0 {
                    return after_name(&cursor, 0);
                }
            }
            // A `;` or a block can't be inside generic arguments.
            Some((';', _)) => return AfterName::Other,
            _ => {}
        }
        after_minus = punct == Some(('-', true));
    }
}

impl Parse for GenericArg {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        // `Name = Type`/`Name<..> = Type` bindings and `Name: Bounds`/
        // `Name<..>: Bounds` constraints.
        let after = if input.peek_ident_str().is_none() {
            AfterName::Other
        } else if input.nth_punct_char(1).is_some_and(|(ch, _)| ch == '<') {
            generic_name_followed_by(input)
        } else {
            after_name(input, 1)
        };
        if after == AfterName::Eq
            && let Ok((ident, generics, eq, ty)) =
                input.try_parse::<(_, Option<Peekable<_>>, _, _)>()
        {
            Ok(Self::Bindings(ident, generics.map(Peekable::inner), eq, ty))
        } else if after == AfterName::Colon
            && let Ok((ident, generics, colon, bounds)) =
                input.try_parse::<(_, Option<Peekable<_>>, _, _)>()
        {
            Ok(Self::Constraint(
                ident,
                generics.map(Peekable::inner),
                colon,
                bounds,
            ))
        } else if input.peek_punct_char().is_some_and(|(ch, _)| ch == '\'')
            && let Ok(lifetime) = input.try_parse()
        {
            Ok(Self::Lifetime(lifetime))
        } else if let Ok(ty) = input.try_parse() {
            Ok(Self::Type(Box::new(ty)))
        } else if let Ok(lifetime) = input.try_parse() {
            Ok(Self::Lifetime(lifetime))
        } else if let Ok(arg) = input.try_parse() {
            Ok(Self::Const(arg))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected a generic argument",
                input.span(),
            ))
        }
    }
}
impl ToTokens for GenericArgs {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.start_token.to_tokens(tokens);
        self.generics.to_tokens(tokens);
        self.last_token.to_tokens(tokens);
    }
}
impl Parse for GenericArgs {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            start_token: input.parse()?,
            generics: input.parse()?,
            last_token: input.parse()?,
        })
    }
}
impl ToTokens for TypePathFn {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.inputs.to_tokens(tokens);
        self.return_type.to_tokens(tokens);
    }
}

impl Parse for TypePathFn {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            inputs: input
                .parse::<Parenthesized<Option<Peekable<_>>>>()?
                .map(|inputs| inputs.map(Peekable::inner)),
            return_type: input.try_parse().ok(),
        })
    }
}
impl ToTokens for TypePathFnInputs {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.args.to_tokens(tokens);
    }
}

impl Parse for TypePathFnInputs {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            args: input.parse()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate as parsyng;
    use parsyng_quote_macros::quote;

    use super::*;
    use crate::ast::tests::check;

    #[test]
    fn test_type_path() {
        check::<TypePathSegment>(quote! {
            Iterator<Item = &Attribute>
        });
    }

    #[test]
    fn test_fn_sugar() {
        check::<Type>(quote! { Box<dyn Fn(u8)> });
        check::<Type>(quote! { Box<dyn Fn(T) -> T + Send> });
        check::<Type>(quote! { Box<dyn FnMut(u8, u16,) -> u8> });
        check::<Type>(quote! { impl Fn(u8) });
        check::<Type>(quote! { Box<dyn for<'a> Fn(&'a u8)> });
        check::<Type>(quote! { F<Fn()> });
    }
}
