//! `#[...]` outer and `#![...]` inner attributes.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Bracketed,
        tokens::{Not, Pound},
    },
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::{Span, TokenStream},
};

/// An attribute, either outer (`#[...]`) or inner (`#![...]`).
///
/// The bracketed content (`meta`) is kept as a raw [`TokenStream`] rather
/// than parsed into a structured "path + arguments" representation.
///
/// Reference: <https://doc.rust-lang.org/reference/attributes.html>
#[derive(Clone, Debug)]
pub struct Attribute {
    pound: Pound,
    bang: Option<Not>,
    meta: Bracketed<TokenStream>,
}

impl Attribute {
    /// `true` for `#![...]` (inner), `false` for `#[...]` (outer).
    #[must_use]
    pub const fn is_inner(&self) -> bool {
        self.bang.is_some()
    }

    /// The span of the bracketed attribute content.
    #[must_use]
    pub fn span(&self) -> Span {
        self.meta.span()
    }
}

impl Parse for Attribute {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            pound: input.parse()?,
            bang: input.try_parse().ok(),
            meta: input.parse()?,
        })
    }
}

impl ToTokens for Attribute {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.pound.to_tokens(tokens);
        self.bang.to_tokens(tokens);
        self.meta.to_tokens(tokens);
    }
}

/// Parse as many leading outer (`#[...]`) attributes as possible.
///
/// Stops (without error) at the first token that isn't one — including at
/// an inner attribute, which is rejected here and left for the caller.
pub fn parse_outer_attributes(input: &mut ParseBuffer) -> Vec<Attribute> {
    let mut attributes = Vec::new();
    while let Ok(attribute) = input.try_advance(|input| {
        let attribute: Attribute = input.parse()?;
        if attribute.is_inner() {
            Err(Diagnostics::new_error_spanned(
                "Expected outer attribute",
                attribute.span(),
            ))
        } else {
            Ok(attribute)
        }
    }) {
        attributes.push(attribute);
    }
    attributes
}

/// Parse as many leading inner (`#![...]`) attributes as possible.
///
/// Stops (without error) at the first token that isn't one — including at
/// an outer attribute, which is rejected here and left for the caller.
pub fn parse_inner_attributes(input: &mut ParseBuffer) -> Vec<Attribute> {
    let mut attributes = Vec::new();
    while let Ok(attribute) = input.try_advance(|input| {
        let attribute: Attribute = input.parse()?;
        if attribute.is_inner() {
            Ok(attribute)
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected inner attribute",
                attribute.span(),
            ))
        }
    }) {
        attributes.push(attribute);
    }
    attributes
}
