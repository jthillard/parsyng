//! `const` items.

use crate::ToTokens;

use crate::ast::tokens::Semicolon;
use crate::ast::{
    expression::Expression,
    item::{GenericParams, WhereClause},
    tokens::{self, Colon, Eq},
    r#type::Type,
};
use crate::parse::Parse;
use crate::proc_macro::Ident;

/// A `const` item: `const NAME: Type = expr;`, or `const NAME: Type;`
/// without a value in a trait. Generic const items (nightly) are covered
/// too: `const NAME<T>: Type = expr where T: Trait;`.
///
/// Does not include leading attributes/visibility — see
/// [`ItemConst`](crate::ast::item::ItemConst) for that.
///
/// Reference: <https://doc.rust-lang.org/reference/items/constant-items.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct ConstantItem {
    const_token: tokens::Const,
    ident: Ident,
    generics: Option<GenericParams>,
    colon: Colon,
    ty: Type,
    default: Option<(Eq, Expression)>,
    where_clause: Option<WhereClause>,
    semi: Semicolon,
}

impl Parse for ConstantItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            const_token: input.parse()?,
            ident: input.parse()?,
            generics: input.try_parse().ok(),
            colon: input.parse()?,
            ty: input.parse()?,
            default: if let Ok(eq) = input.peek_parse() {
                Some((eq, input.parse()?))
            } else {
                None
            },
            where_clause: input.try_parse().ok(),
            semi: input.parse()?,
        })
    }
}

impl ToTokens for ConstantItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.const_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.generics.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.default.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
        self.semi.to_tokens(tokens);
    }
}
