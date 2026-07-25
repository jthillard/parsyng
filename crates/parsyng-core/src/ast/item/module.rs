//! `mod` items.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Braced,
        tokens::{Mod, Semicolon},
    },
    parse::Parse,
    proc_macro::{Delimiter, Ident, TokenStream},
};

/// A `mod` item: `mod foo;` (external file) or `mod foo { ... }` (inline).
///
/// For `mod foo;`, `content` is `None`. For the inline form, the body is
/// kept as raw, unparsed tokens rather than recursively parsed into
/// [`Item`](crate::ast::item::Item)s. Does not include leading
/// attributes/visibility — see [`ItemMod`](crate::ast::item::ItemMod) for
/// that.
///
/// Reference: <https://doc.rust-lang.org/reference/items/modules.html>
#[derive(Clone, Debug)]
pub struct ModItem {
    mod_token: Mod,
    ident: Ident,
    content: Option<Braced<TokenStream>>,
    semi: Option<Semicolon>,
}

impl Parse for ModItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let mod_token = input.parse()?;
        let ident = input.parse()?;
        let (content, semi) = if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Brace
        {
            (Some(input.parse()?), None)
        } else {
            (None, Some(input.parse()?))
        };

        Ok(Self {
            mod_token,
            ident,
            content,
            semi,
        })
    }
}

impl ToTokens for ModItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.mod_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.content.to_tokens(tokens);
        self.semi.to_tokens(tokens);
    }
}
