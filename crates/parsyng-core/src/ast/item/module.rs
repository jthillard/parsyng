//! `mod` items.

use crate::ToTokens;

use crate::{
    ast::{
        crate_source::Crate,
        delimiter::Braced,
        tokens::{Mod, Semicolon, Unsafe},
    },
    parse::Parse,
    proc_macro::{Delimiter, Ident},
};

/// A `mod` item: `mod foo;` (external file) or `mod foo { ... }` (inline).
///
/// For `mod foo;`, `content` is `None`. The inline form's body is parsed
/// like a whole source file, as a [`Crate`]: inner attributes, then items.
/// Does not include leading
/// attributes/visibility — see [`ItemMod`](crate::ast::item::ItemMod) for
/// that.
///
/// Reference: <https://doc.rust-lang.org/reference/items/modules.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct ModItem {
    unsafety: Option<Unsafe>,
    mod_token: Mod,
    ident: Ident,
    content: Option<Braced<Crate>>,
    semi: Option<Semicolon>,
}

impl Parse for ModItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let unsafety = input.try_parse().ok();
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
            unsafety,
            mod_token,
            ident,
            content,
            semi,
        })
    }
}

impl ToTokens for ModItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.unsafety.to_tokens(tokens);
        self.mod_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.content.to_tokens(tokens);
        self.semi.to_tokens(tokens);
    }
}
