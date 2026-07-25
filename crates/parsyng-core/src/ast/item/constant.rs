//! `const` items.

use crate::ToTokens;

use crate::ast::tokens::Semicolon;
use crate::ast::{
    token_stream::TokenStreamUntilSemicolon,
    tokens::{self, Colon, Eq},
    r#type::Type,
};
use crate::parse::Parse;
use crate::proc_macro::Ident;

/// A `const` item: `const NAME: Type = expr;` (the default-value expression
/// is kept as raw, unparsed tokens).
///
/// Does not include leading attributes/visibility — see
/// [`ItemConst`](crate::ast::item::ItemConst) for that.
///
/// Reference: <https://doc.rust-lang.org/reference/items/constant-items.html>
#[derive(Clone, Debug)]
pub struct ConstantItem {
    const_token: tokens::Const,
    ident: Ident,
    colon: Colon,
    ty: Type,
    default: Option<(Eq, TokenStreamUntilSemicolon)>,
    semi: Semicolon,
}

impl Parse for ConstantItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            const_token: input.parse()?,
            ident: input.parse()?,
            colon: input.parse()?,
            ty: input.parse()?,
            default: input.try_parse().ok(),
            semi: input.parse()?,
        })
    }
}

impl ToTokens for ConstantItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.const_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.default.to_tokens(tokens);
        self.semi.to_tokens(tokens);
    }
}
