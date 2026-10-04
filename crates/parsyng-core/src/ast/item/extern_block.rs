//! `extern` blocks.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Braced,
        tokens::{Extern, Unsafe},
    },
    parse::Parse,
    proc_macro::{Literal, TokenStream},
};

/// An `extern` block, without its leading attributes/visibility (see
/// [`ItemExternBlock`](crate::ast::item::ItemExternBlock) for that):
/// `unsafe extern "C" { ... }` (the body is kept as raw, unparsed tokens).
///
/// Reference: <https://doc.rust-lang.org/reference/items/external-blocks.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct ExternBlockItem {
    unsafe_token: Option<Unsafe>,
    extern_token: Extern,
    abi: Option<Literal>,
    items: Braced<TokenStream>,
}

impl Parse for ExternBlockItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            unsafe_token: input.try_parse().ok(),
            extern_token: input.parse()?,
            abi: input.try_parse().ok(),
            items: input.parse()?,
        })
    }
}

impl ToTokens for ExternBlockItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.unsafe_token.to_tokens(tokens);
        self.extern_token.to_tokens(tokens);
        self.abi.to_tokens(tokens);
        self.items.to_tokens(tokens);
    }
}
