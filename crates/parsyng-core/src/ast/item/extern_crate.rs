//! `extern crate` items.

use crate::ToTokens;

use crate::{
    ast::tokens::{As, Crate, Extern, Semicolon},
    parse::Parse,
    proc_macro::Ident,
};

/// An `extern crate` item, without its leading attributes/visibility (see
/// [`ItemExternCrate`](crate::ast::item::ItemExternCrate) for that): `extern
/// crate foo as bar;`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/extern-crates.html>
#[derive(Clone, Debug)]
pub struct ExternCrateItem {
    extern_token: Extern,
    crate_token: Crate,
    ident: Ident,
    rename: Option<(As, Ident)>,
    semi: Semicolon,
}

impl Parse for ExternCrateItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            extern_token: input.parse()?,
            crate_token: input.parse()?,
            ident: input.parse()?,
            rename: input.try_parse().ok(),
            semi: input.parse()?,
        })
    }
}

impl ToTokens for ExternCrateItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.extern_token.to_tokens(tokens);
        self.crate_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.rename.to_tokens(tokens);
        self.semi.to_tokens(tokens);
    }
}
