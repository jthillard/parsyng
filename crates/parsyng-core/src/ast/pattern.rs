//! Patterns, e.g. the `pat` in `let pat = ...`, a function parameter, or a
//! `match` arm.
//!
//! Coverage: binding (`ref mut name`), wildcard (`_`), tuple, reference
//! (`&`/`&mut`), literal (numeric only — matches the same gap documented on
//! [`ast::literal::Literal`](crate::ast::literal::Literal)), path
//! (`Foo::Bar`), tuple-struct (`Foo(a, b)`), struct (`Foo { a, b: pat, ..
//! }`), rest (`..`), and `|` alternation. Not covered: slice patterns,
//! range patterns (`1..=5`), and box patterns.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::{Braced, Parenthesized},
        literal::Literal,
        path::SimplePath,
        tokens::{And, Colon, Comma, DotDot, Minus, Mut, Or, Ref},
    },
    combinator::Punctuated,
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::{Delimiter, Ident},
};

/// A pattern. See the [module docs](self) for coverage.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html>
#[derive(Clone, Debug)]
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
    /// A numeric literal pattern, e.g. `1`, `-1.5`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#literal-patterns>
    Literal(PatLiteral),
    /// A multi-segment path pattern, e.g. `Foo::Bar` (a unit enum variant or
    /// constant). A single-segment path parses as [`Ident`](Self::Ident)
    /// instead — see [`Pattern::parse`].
    ///
    /// Reference: <https://doc.rust-lang.org/reference/patterns.html#path-patterns>
    Path(PatPath),
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

/// A binding pattern, e.g. `ref mut name`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#identifier-patterns>
#[derive(Clone, Debug)]
pub struct PatIdent {
    by_ref: Option<Ref>,
    mutability: Option<Mut>,
    ident: Ident,
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
        if let Some(ident) = input.peek_ident() {
            #[allow(clippy::cmp_owned)]
            let is_ref_or_mut = ident.to_string() == "ref" || ident.to_string() == "mut";
            if is_ref_or_mut {
                return Ok(Self::Ident(input.parse()?));
            }
        }
        let path: SimplePath = input.parse()?;
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
            }));
        }
        Ok(Self::Path(PatPath { path }))
    }
}

/// The wildcard pattern `_`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#wildcard-pattern>
#[derive(Clone, Debug)]
pub struct PatWildcard {
    underscore: Ident,
}

/// A tuple pattern, e.g. `(a, b, c)`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#tuple-patterns>
#[derive(Clone, Debug)]
pub struct PatTuple {
    elems: Parenthesized<Punctuated<Pattern, Comma>>,
}

/// A reference pattern, e.g. `&mut pat`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#reference-patterns>
#[derive(Clone, Debug)]
pub struct PatRef {
    and_token: And,
    mutability: Option<Mut>,
    pat: Box<Pattern>,
}

/// A numeric literal pattern, e.g. `1`, `-1.5`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#literal-patterns>
#[derive(Clone, Debug)]
pub struct PatLiteral {
    neg: Option<Minus>,
    literal: Literal,
}

/// A multi-segment path pattern, e.g. `Foo::Bar`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#path-patterns>
#[derive(Clone, Debug)]
pub struct PatPath {
    path: SimplePath,
}

/// A tuple-struct pattern: `Path(a, b, ..)`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#tuple-struct-patterns>
#[derive(Clone, Debug)]
pub struct PatTupleStruct {
    path: SimplePath,
    elems: Parenthesized<Punctuated<Pattern, Comma>>,
}

/// A struct pattern: `Path { a, b: pat, .. }`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#struct-patterns>
#[derive(Clone, Debug)]
pub struct PatStruct {
    path: SimplePath,
    fields: Braced<Punctuated<StructPatternField, Comma>>,
}

/// One field inside a [`PatStruct`]'s braces.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#struct-patterns>
#[derive(Clone, Debug)]
pub enum StructPatternField {
    /// `field: pattern`.
    Named(Ident, Colon, Pattern),
    /// `ref? mut? field` shorthand.
    Shorthand(PatIdent),
    /// `..`, matching (and ignoring) any remaining fields.
    Rest(DotDot),
}

/// The rest pattern `..`.
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#rest-patterns>
#[derive(Clone, Debug)]
pub struct PatRest {
    dot_dot: DotDot,
}

/// `pat | pat | ...` (at least two alternatives).
///
/// Reference: <https://doc.rust-lang.org/reference/patterns.html#or-patterns>
#[derive(Clone, Debug)]
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
        })
    }
}

impl Parse for PatWildcard {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let underscore: Ident = input.parse()?;
        #[allow(clippy::cmp_owned)]
        if underscore.to_string() == "_" {
            Ok(Self { underscore })
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected `_`",
                underscore.span(),
            ))
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
        // literals inside `quote!` are wrapped in an invisible group (see
        // `ast::tests::literal_nodes`), so use a raw string here instead.
        let lit = check::<Pattern>("-1".parse().unwrap());
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
}
