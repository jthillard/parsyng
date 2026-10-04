//! `static` items.

use crate::ToTokens;

use crate::{
    ast::{
        expression::Expression,
        tokens::{self, Colon, Eq, Mut, Semicolon},
        r#type::Type,
    },
    parse::Parse,
    proc_macro::Ident,
};

/// A `static` item: `static mut NAME: Type = expr;`, or `static NAME:
/// Type;` without a value in an `extern` block.
///
/// Does not include leading attributes/visibility — see
/// [`ItemStatic`](crate::ast::item::ItemStatic) for that.
///
/// Reference: <https://doc.rust-lang.org/reference/items/static-items.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct StaticItem {
    static_token: tokens::Static,
    mut_token: Option<Mut>,
    ident: Ident,
    colon: Colon,
    ty: Type,
    default: Option<(Eq, Expression)>,
    semi: Semicolon,
}

impl Parse for StaticItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            static_token: input.parse()?,
            mut_token: input.try_parse().ok(),
            ident: input.parse()?,
            colon: input.parse()?,
            ty: input.parse()?,
            default: if let Ok(eq) = input.peek_parse() {
                Some((eq, input.parse()?))
            } else {
                None
            },
            semi: input.parse()?,
        })
    }
}

impl ToTokens for StaticItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.static_token.to_tokens(tokens);
        self.mut_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.default.to_tokens(tokens);
        self.semi.to_tokens(tokens);
    }
}
