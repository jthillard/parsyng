//! A whole source file: leading inner attributes followed by a list of items.

use crate::ToTokens;

use crate::{
    Parse,
    ast::{attributes::parse_inner_attributes, item::Item},
};

/// The contents of an entire `.rs` file: any number of leading `#![...]`
/// inner attributes, followed by any number of [`Item`]s.
///
/// This is the top-level entry point for parsing a complete module or crate
/// root, as opposed to [`item::DeriveInput`](crate::ast::item::DeriveInput)
/// (a single struct/enum, for `#[derive(...)]` macros) or a single
/// [`Item`] (for `#[proc_macro_attribute]` macros applied to one item).
///
/// Reference: <https://doc.rust-lang.org/reference/crates-and-source-files.html>
#[derive(Clone, Debug)]
pub struct Crate {
    inner_attributes: Vec<crate::ast::attributes::Attribute>,
    items: Vec<Item>,
}

impl Parse for Crate {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let inner_attributes = parse_inner_attributes(input);

        let mut items = Vec::new();
        while !input.is_empty() {
            items.push(input.parse::<crate::ast::item::Item>()?);
        }

        Ok(Self {
            inner_attributes,
            items,
        })
    }
}

impl ToTokens for Crate {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.inner_attributes.to_tokens(tokens);
        self.items.to_tokens(tokens);
    }
}
