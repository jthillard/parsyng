//! `extern` blocks.

use crate::ToTokens;

use crate::{
    ast::{delimiter::Braced, tokens::Extern},
    parse::Parse,
    proc_macro::{Literal, TokenStream},
};

/// An `extern` block, without its leading attributes/visibility (see
/// [`ItemExternBlock`](crate::ast::item::ItemExternBlock) for that): `extern
/// "C" { ... }` (the body is kept as raw, unparsed tokens).
///
/// Reference: <https://doc.rust-lang.org/reference/items/external-blocks.html>
#[derive(Clone, Debug)]
pub struct ExternBlockItem {
    extern_token: Extern,
    abi: Option<Literal>,
    items: Braced<TokenStream>,
}

impl Parse for ExternBlockItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            extern_token: input.parse()?,
            abi: input.try_parse().ok(),
            items: input.parse()?,
        })
    }
}

impl ToTokens for ExternBlockItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.extern_token.to_tokens(tokens);
        self.abi.to_tokens(tokens);
        self.items.to_tokens(tokens);
    }
}
