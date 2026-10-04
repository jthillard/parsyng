//! Patterns, e.g. the `pat` in `let pat = ...`, a function parameter, or a
//! `match` arm.
//!
//! Coverage: binding (`ref mut name`), wildcard (`_`), tuple, reference
//! (`&`/`&mut`), literal (any
//! [`ast::literal::Literal`](crate::ast::literal::Literal), optionally
//! negated), path
//! (`Foo::Bar`), qualified path (`<T as Trait>::CONST`), tuple-struct
//! (`Foo(a, b)`), struct (`Foo { a, 0: b, c: pat, .. }`), macro invocation
//! (`m!(x)`), rest (`..`), and `|` alternation. Not covered: slice
//! patterns, range patterns (`1..=5`), and box patterns.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::{Braced, Parenthesized},
        item::macro_item::MacroInvocationItem,
        literal::{Literal, LiteralNumber},
        path::SimplePath,
        tokens::{And, At, Colon, Comma, DotDot, Minus, Mut, Or, Ref},
        r#type::TypeQualifiedPath,
    },
    combinator::Punctuated,
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::{Delimiter, Ident},
};

/// A pattern. See the [module docs](self) for coverage.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum Pattern {
    /// A binding pattern, e.g. `ref mut name`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#identifier-patterns>
    Ident(PatIdent),
    /// The wildcard pattern `_`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#wildcard-pattern>
    Wildcard(PatWildcard),
    /// A tuple pattern: `(a, b, c)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#tuple-patterns>
    Tuple(Box<PatTuple>),
    /// A reference pattern: `&mut pat`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#reference-patterns>
    Ref(PatRef),
    /// A literal pattern, e.g. `1`, `-1.5`, `"foo"` or `b'a'`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#literal-patterns>
    Literal(PatLiteral),
    /// A multi-segment path pattern, e.g. `Foo::Bar` (a unit enum variant or
    /// constant). A single-segment path parses as [`Ident`](Self::Ident)
    /// instead — see [`Pattern::parse`].
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#path-patterns>
    Path(PatPath),
    /// A qualified path pattern, e.g. `<T as Trait>::CONST` or `<T>::CONST`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#path-patterns>
    QualifiedPath(Box<TypeQualifiedPath>),
    /// A macro invocation in pattern position, e.g. `m!(x)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
    Macro(MacroInvocationItem),
    /// A tuple-struct pattern: `Path(a, b, ..)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#tuple-struct-patterns>
    TupleStruct(Box<PatTupleStruct>),
    /// A struct pattern: `Path { a, b: pat, .. }`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#struct-patterns>
    Struct(Box<PatStruct>),
    /// The rest pattern `..`, e.g. inside `(a, .., b)`, `Path(a, ..)`, or
    /// `Path { a, .. }`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#rest-patterns>
    Rest(PatRest),
    /// `pat | pat | ...` (at least two alternatives).
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#or-patterns>
    Or(Box<PatOr>),
}

/// A binding pattern, e.g. `ref mut name` or `name @ Some(_)`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#identifier-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatIdent {
    by_ref: Option<Ref>,
    mutability: Option<Mut>,
    ident: Ident,
    subpattern: Option<(At, Box<Pattern>)>,
}

/// The `@ pattern` of a binding (a pattern without top-level `|`: `x @ A |
/// B` is `(x @ A) | B`).
fn parse_subpattern(input: &mut ParseBuffer) -> crate::error::Result<Option<(At, Box<Pattern>)>> {
    if input.peek_punct_char().is_some_and(|(ch, _)| ch == '@') {
        Ok(Some((
            input.parse()?,
            Box::new(Pattern::parse_atom(input)?),
        )))
    } else {
        Ok(None)
    }
}

impl Pattern {
    /// The bound identifier, for [`Ident`](Self::Ident)/[`Wildcard`](Self::Wildcard)
    /// (recursing through [`Ref`](Self::Ref)); `None` for every other variant.
    #[must_use]
    pub fn ident(&self) -> Option<&Ident> {
        match self {
            Self::Ident(pat_ident) => Some(&pat_ident.ident),
            Self::Wildcard(pat_wildcard) => Some(&pat_wildcard.underscore),
            Self::Ref(pat_ref) => pat_ref.pat.ident(),
            Self::Tuple(_)
            | Self::Literal(_)
            | Self::Path(_)
            | Self::QualifiedPath(_)
            | Self::Macro(_)
            | Self::TupleStruct(_)
            | Self::Struct(_)
            | Self::Rest(_)
            | Self::Or(_) => None,
        }
    }
    /// The `mut` token, if this pattern (or, recursing through
    /// [`Ref`](Self::Ref), the pattern it wraps) is mutable.
    #[must_use]
    pub fn mutability(&self) -> Option<&Mut> {
        match self {
            Self::Ident(pat_ident) => pat_ident.mutability.as_ref(),
            Self::Ref(pat_ref) => pat_ref.pat.mutability(),
            Self::Wildcard(_)
            | Self::Tuple(_)
            | Self::Literal(_)
            | Self::Path(_)
            | Self::QualifiedPath(_)
            | Self::Macro(_)
            | Self::TupleStruct(_)
            | Self::Struct(_)
            | Self::Rest(_)
            | Self::Or(_) => None,
        }
    }
    /// Parse a pattern without top-level `|` alternation (`PatternNoTopAlt`
    /// in the Rust reference grammar) — needed wherever a trailing `|`
    /// could instead mean something else entirely, e.g. a closure
    /// parameter's pattern, where `|x| body` must not let `x`'s pattern
    /// parsing eat the closure's own closing `|` as an or-pattern
    /// separator. Nowhere else needs this: `match`/`let`/`for` patterns
    /// aren't followed by a bare `|`, so they use the full,
    /// alternation-capable [`Pattern::parse`] instead.
    pub(crate) fn parse_no_top_alt(input: &mut ParseBuffer) -> crate::error::Result<Self> {
        Self::parse_atom(input)
    }

    /// Parse a single pattern, i.e. anything but the top-level `|`
    /// alternation handled by [`PatOr`]/[`Pattern::parse`] — used as the
    /// building block for alternatives and for nested sub-patterns (tuple
    /// elements, struct/tuple-struct fields, `&pat`) that don't themselves
    /// need another layer of alternation.
    fn parse_atom(input: &mut ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(rest) = input.try_parse() {
            return Ok(Self::Rest(rest));
        }
        if let Ok(reference) = input.try_parse() {
            return Ok(Self::Ref(reference));
        }
        if let Ok(tuple) = input.try_parse() {
            return Ok(Self::Tuple(Box::new(tuple)));
        }
        if let Ok(literal) = input.try_parse() {
            return Ok(Self::Literal(literal));
        }
        if matches!(input.peek_ident_str(), Some("ref" | "mut")) {
            return Ok(Self::Ident(input.parse()?));
        }
        if input.peek_punct_char().is_some_and(|(ch, _)| ch == '<') {
            return Ok(Self::QualifiedPath(Box::new(
                TypeQualifiedPath::parse_expression(input)?,
            )));
        }
        let mut fork = input.clone();
        let path: SimplePath = fork.parse()?;
        if fork.peek_punct_char().is_some_and(|(ch, _)| ch == '!') {
            return MacroInvocationItem::parse_without_semicolon(input).map(Self::Macro);
        }
        *input = fork;
        if let Some(group) = input.peek_group() {
            if group.delimiter() == Delimiter::Parenthesis {
                return Ok(Self::TupleStruct(Box::new(PatTupleStruct {
                    elems: input.parse()?,
                    path,
                })));
            }
            if group.delimiter() == Delimiter::Brace {
                return Ok(Self::Struct(Box::new(PatStruct {
                    fields: input.parse()?,
                    path,
                })));
            }
        }
        if let Some(ident) = path.as_single_ident() {
            #[allow(clippy::cmp_owned)]
            if ident.to_string() == "_" {
                return Ok(Self::Wildcard(PatWildcard {
                    underscore: ident.clone(),
                }));
            }
            return Ok(Self::Ident(PatIdent {
                by_ref: None,
                mutability: None,
                ident: ident.clone(),
                subpattern: parse_subpattern(input)?,
            }));
        }
        Ok(Self::Path(PatPath { path }))
    }
}

/// The wildcard pattern `_`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#wildcard-pattern>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatWildcard {
    underscore: Ident,
}

/// A tuple pattern, e.g. `(a, b, c)`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#tuple-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatTuple {
    elems: Parenthesized<Punctuated<Pattern, Comma>>,
}

/// A reference pattern, e.g. `&mut pat`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#reference-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatRef {
    and_token: And,
    mutability: Option<Mut>,
    pat: Box<Pattern>,
}

/// A literal pattern, e.g. `1`, `-1.5`, `"foo"` or `b'a'`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#literal-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatLiteral {
    neg: Option<Minus>,
    literal: Literal,
}

/// A multi-segment path pattern, e.g. `Foo::Bar`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#path-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatPath {
    path: SimplePath,
}

/// A tuple-struct pattern: `Path(a, b, ..)`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#tuple-struct-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatTupleStruct {
    path: SimplePath,
    elems: Parenthesized<Punctuated<Pattern, Comma>>,
}

/// A struct pattern: `Path { a, b: pat, .. }`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#struct-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatStruct {
    path: SimplePath,
    fields: Braced<Punctuated<StructPatternField, Comma>>,
}

/// One field inside a [`PatStruct`]'s braces.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#struct-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum StructPatternField {
    /// `field: pattern`.
    Named(Ident, Colon, Pattern),
    /// `0: pattern`, for a tuple struct's field.
    Unnamed(LiteralNumber, Colon, Pattern),
    /// `ref? mut? field` shorthand.
    Shorthand(PatIdent),
    /// `..`, matching (and ignoring) any remaining fields.
    Rest(DotDot),
}

/// The rest pattern `..`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#rest-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatRest {
    dot_dot: DotDot,
}

/// `pat | pat | ...` (at least two alternatives).
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#or-patterns>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct PatOr {
    first: Pattern,
    alternatives: Vec<(Or, Pattern)>,
}

impl Parse for Pattern {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(or) = input.try_parse::<PatOr>() {
            Ok(Self::Or(Box::new(or)))
        } else {
            Self::parse_atom(input)
        }
    }
}

impl Parse for PatIdent {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            by_ref: input.try_parse().ok(),
            mutability: input.try_parse().ok(),
            ident: input.parse()?,
            subpattern: parse_subpattern(input)?,
        })
    }
}

impl Parse for PatWildcard {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Some(underscore) = input.ident_str_and(|text| text == "_") {
            Ok(Self { underscore })
        } else {
            Err(Diagnostics::new_error_spanned("Expected `_`", input.span()))
        }
    }
}

impl Parse for PatTuple {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            elems: input.parse()?,
        })
    }
}

impl Parse for PatRef {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            and_token: input.parse()?,
            mutability: input.try_parse().ok(),
            pat: Box::new(input.parse()?),
        })
    }
}

impl Parse for PatLiteral {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            neg: input.try_parse().ok(),
            literal: input.parse()?,
        })
    }
}

impl Parse for PatPath {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            path: input.parse()?,
        })
    }
}

impl Parse for PatTupleStruct {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            path: input.parse()?,
            elems: input.parse()?,
        })
    }
}

impl Parse for PatStruct {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            path: input.parse()?,
            fields: input.parse()?,
        })
    }
}

impl Parse for StructPatternField {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(rest) = input.try_parse() {
            return Ok(Self::Rest(rest));
        }
        if let Ok((ident, colon, pat)) = input.try_parse::<(Ident, Colon, Pattern)>() {
            return Ok(Self::Named(ident, colon, pat));
        }
        if input.peek_literal().is_some() {
            let (index, colon, pat) = input.parse::<(LiteralNumber, Colon, Pattern)>()?;
            return Ok(Self::Unnamed(index, colon, pat));
        }
        Ok(Self::Shorthand(input.parse()?))
    }
}

impl Parse for PatRest {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            dot_dot: input.parse()?,
        })
    }
}

impl Parse for PatOr {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let first = Pattern::parse_atom(input)?;
        let mut alternatives = Vec::new();
        while let Ok(pipe) = input.try_parse::<Or>() {
            alternatives.push((pipe, Pattern::parse_atom(input)?));
        }
        if alternatives.is_empty() {
            Err(Diagnostics::new_error_spanned(
                "Expected `|` after pattern",
                input.span(),
            ))
        } else {
            Ok(Self {
                first,
                alternatives,
            })
        }
    }
}

impl ToTokens for Pattern {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Ident(ident) => ident.to_tokens(tokens),
            Self::Wildcard(wildcard) => wildcard.to_tokens(tokens),
            Self::Tuple(tuple) => tuple.to_tokens(tokens),
            Self::Ref(reference) => reference.to_tokens(tokens),
            Self::Literal(literal) => literal.to_tokens(tokens),
            Self::Path(path) => path.to_tokens(tokens),
            Self::QualifiedPath(path) => path.to_tokens(tokens),
            Self::Macro(invocation) => invocation.to_tokens(tokens),
            Self::TupleStruct(tuple_struct) => tuple_struct.to_tokens(tokens),
            Self::Struct(r#struct) => r#struct.to_tokens(tokens),
            Self::Rest(rest) => rest.to_tokens(tokens),
            Self::Or(or) => or.to_tokens(tokens),
        }
    }
}

impl ToTokens for PatIdent {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.by_ref.to_tokens(tokens);
        self.mutability.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        if let Some((at, subpattern)) = &self.subpattern {
            at.to_tokens(tokens);
            subpattern.to_tokens(tokens);
        }
    }
}

impl ToTokens for PatWildcard {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.underscore.to_tokens(tokens);
    }
}

impl ToTokens for PatTuple {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.elems.to_tokens(tokens);
    }
}

impl ToTokens for PatRef {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.and_token.to_tokens(tokens);
        self.mutability.to_tokens(tokens);
        self.pat.to_tokens(tokens);
    }
}

impl ToTokens for PatLiteral {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.neg.to_tokens(tokens);
        self.literal.to_tokens(tokens);
    }
}

impl ToTokens for PatPath {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path.to_tokens(tokens);
    }
}

impl ToTokens for PatTupleStruct {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path.to_tokens(tokens);
        self.elems.to_tokens(tokens);
    }
}

impl ToTokens for PatStruct {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path.to_tokens(tokens);
        self.fields.to_tokens(tokens);
    }
}

impl ToTokens for StructPatternField {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Named(ident, colon, pat) => {
                ident.to_tokens(tokens);
                colon.to_tokens(tokens);
                pat.to_tokens(tokens);
            }
            Self::Unnamed(index, colon, pat) => {
                index.to_tokens(tokens);
                colon.to_tokens(tokens);
                pat.to_tokens(tokens);
            }
            Self::Shorthand(ident) => ident.to_tokens(tokens),
            Self::Rest(dot_dot) => dot_dot.to_tokens(tokens),
        }
    }
}

impl ToTokens for PatRest {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.dot_dot.to_tokens(tokens);
    }
}

impl ToTokens for PatOr {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.first.to_tokens(tokens);
        for (pipe, pat) in &self.alternatives {
            pipe.to_tokens(tokens);
            pat.to_tokens(tokens);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate as parsyng;
    use parsyng_quote_macros::quote;

    use super::*;
    use crate::ast::tests::check;

    #[test]
    fn test_pattern_ident() {
        let ident = check::<Pattern>(quote! { ref mut name });
        assert!(matches!(ident, Pattern::Ident(_)));
    }

    #[test]
    fn test_pattern_wildcard() {
        let wildcard = check::<Pattern>(quote! { _ });
        assert!(matches!(wildcard, Pattern::Wildcard(_)));
    }

    #[test]
    fn test_pattern_tuple() {
        check::<Pattern>(quote! { (a, &mut b) });
    }

    #[test]
    fn test_pattern_ref() {
        check::<Pattern>(quote! { &mut _ });
    }

    #[test]
    fn test_pattern_literal() {
        let lit = check::<Pattern>(quote! { -1 });
        assert!(matches!(lit, Pattern::Literal(_)));
        let lit = check::<Pattern>(quote! { "foo" });
        assert!(matches!(lit, Pattern::Literal(_)));
    }

    #[test]
    fn test_pattern_path() {
        let path = check::<Pattern>(quote! { Foo::Bar });
        assert!(matches!(path, Pattern::Path(_)));
    }

    #[test]
    fn test_pattern_tuple_struct() {
        let tuple_struct = check::<Pattern>(quote! { Foo(a, ..) });
        assert!(matches!(tuple_struct, Pattern::TupleStruct(_)));
    }

    #[test]
    fn test_pattern_struct() {
        let r#struct = check::<Pattern>(quote! { Foo { a, b: c, .. } });
        assert!(matches!(r#struct, Pattern::Struct(_)));
    }

    #[test]
    fn test_pattern_rest() {
        let rest = check::<Pattern>(quote! { .. });
        assert!(matches!(rest, Pattern::Rest(_)));
    }

    #[test]
    fn test_pattern_or() {
        let or = check::<Pattern>("1 | 2 | 3".parse().unwrap());
        assert!(matches!(or, Pattern::Or(_)));
    }

    #[test]
    fn test_pattern_tuple_rest() {
        check::<Pattern>(quote! { (a, .., b) });
    }

    #[test]
    fn test_pattern_plain_ident() {
        let plain = check::<Pattern>(quote! { name });
        assert!(matches!(plain, Pattern::Ident(_)));
    }

    #[test]
    fn test_pattern_ident_accessors() {
        let by_ref = check::<Pattern>(quote! { ref x });
        assert_eq!(by_ref.ident().unwrap().to_string(), "x");
        assert!(by_ref.mutability().is_none());

        let by_mut = check::<Pattern>(quote! { mut x });
        assert!(by_mut.mutability().is_some());

        let through_ref = check::<Pattern>(quote! { &mut x });
        assert_eq!(through_ref.ident().unwrap().to_string(), "x");

        let tuple = check::<Pattern>(quote! { (x, y) });
        assert!(tuple.ident().is_none());
    }

    #[test]
    fn test_pattern_nested() {
        check::<Pattern>(quote! { Some(Ok((a, _))) });
        check::<Pattern>(quote! { Foo { a: Bar(b, ..), c: (d, e), ref mut f } });
        check::<Pattern>(quote! { &&(a, &b) });
        check::<Pattern>(quote! { (Some(1 | 2), None) });
        check::<Pattern>(quote! { Some(A | B) | None });
        check::<Pattern>(quote! { () });
        check::<Pattern>(quote! { (a,) });
        check::<Pattern>(quote! { Foo {} });
        check::<Pattern>(quote! { r#type });
        check::<Pattern>(quote! { a::B::C { 0: x, 1: (y, _), .. } });
        check::<Pattern>(quote! { Tuple { 0: ref a, 1: _ } });
    }

    #[test]
    fn test_pattern_macro() {
        let invocation = check::<Pattern>(quote! { m!(x) });
        assert!(matches!(invocation, Pattern::Macro(_)));
        check::<Pattern>(quote! { a::m![1, 2] });
        check::<Pattern>(quote! { Some(m! { x }) | None });
        // A macro pattern doesn't take the `;` after it.
        check::<crate::ast::statements::Statement>(quote! { let m!(x) = y; });
    }

    #[test]
    fn test_pattern_literals() {
        for literal in [
            quote! { 'c' },
            quote! { b'c' },
            quote! { 1.5 },
            quote! { -1.5 },
            quote! { b"bytes" },
            quote! { true },
            quote! { false },
        ] {
            check::<Pattern>(literal);
        }
    }

    #[test]
    fn test_pattern_paths() {
        let qualified = check::<Pattern>(quote! { <T as Trait>::CONST });
        assert!(matches!(qualified, Pattern::QualifiedPath(_)));
        check::<Pattern>(quote! { <T>::CONST });
        check::<Pattern>(quote! { <Vec<u8> as Trait>::A::B });
        check::<Pattern>(quote! { Some(<T as Trait>::CONST) | None });
        check::<Pattern>(quote! { Self::Variant(x) });
        check::<Pattern>(quote! { ::a::B });
        check::<Pattern>(quote! { crate::A { x } });
    }

    #[test]
    fn test_pattern_binding() {
        let binding = check::<Pattern>(quote! { x @ Some(_) });
        assert_eq!(binding.ident().unwrap().to_string(), "x");
        check::<Pattern>(quote! { ref mut x @ (a, b) });
        check::<Pattern>(quote! { (x @ _, y) });
        check::<Pattern>(quote! { Foo { a: n @ Some(_), .. } });
        // `@` binds tighter than `|`.
        let or = check::<Pattern>(quote! { x @ A | B });
        assert!(matches!(or, Pattern::Or(_)));
    }
}
