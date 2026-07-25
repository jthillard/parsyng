//! `trait` items.

use crate::ToTokens;

use crate::{
    ast::{
        attributes::parse_outer_attributes,
        delimiter::Braced,
        item::TypeParamBounds,
        item::{associated::TypeAlias, constant::ConstantItem},
        signature::FnSignature,
        tokens::{Auto, Colon, Trait, Unsafe},
    },
    error::Diagnostics,
    parse::Parse,
    proc_macro::{Delimiter, Ident, TokenStream},
};

/// A `trait` item, without its leading attributes/visibility (see
/// [`ItemTrait`](crate::ast::item::ItemTrait) for that): `unsafe auto trait
/// Foo<T>: Bound where ... { ... }`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/traits.html>
#[derive(Clone, Debug)]
pub struct TraitItem {
    unsafety: Option<Unsafe>,
    auto_token: Option<Auto>,
    trait_token: Trait,
    ident: Ident,
    generics: Option<crate::ast::item::GenericParams>,
    bounds: Option<(Colon, TypeParamBounds)>,
    where_clause: Option<crate::ast::item::WhereClause>,
    items: Braced<Vec<TraitItemMember>>,
}

/// One member inside a [`TraitItem`]'s body.
#[derive(Clone, Debug)]
pub struct TraitItemMember {
    attributes: Vec<crate::ast::attributes::Attribute>,
    kind: TraitItemKind,
}

/// A [`TraitItemMember`]'s kind: an associated type, associated const, or
/// method (with an optional default body).
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html>
#[derive(Clone, Debug)]
pub enum TraitItemKind {
    /// An associated type.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-types>
    Type(Box<TypeAlias>),
    /// An associated const.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-constants>
    Const(Box<ConstantItem>),
    /// A method, with an optional default body.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-functions-and-methods>
    Function(Box<TraitFunction>),
}

/// A trait method declaration, with an optional default body.
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-functions-and-methods>
#[derive(Clone, Debug)]
pub struct TraitFunction {
    signature: FnSignature,
    body: TraitFunctionBody,
}

/// A [`TraitFunction`]'s body: a default `{ ... }` implementation (kept as a
/// raw, unparsed [`TokenStream`]), or a bare `;` (no default).
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-functions-and-methods>
#[derive(Clone, Debug)]
pub enum TraitFunctionBody {
    /// `{ ... }`.
    Block(Braced<TokenStream>),
    /// A bare `;` (no default).
    Semicolon(crate::ast::tokens::Semicolon),
}

impl Parse for TraitItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let unsafety = input.try_parse().ok();
        let auto_token = input.try_parse().ok();
        Ok(Self {
            unsafety,
            auto_token,
            trait_token: input.parse()?,
            ident: input.parse()?,
            generics: input.try_parse().ok(),
            bounds: input.try_parse().ok(),
            where_clause: input.try_parse().ok(),
            items: input.parse()?,
        })
    }
}

impl Parse for TraitItemMember {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        let kind = if let Ok(item) = input.try_parse() {
            TraitItemKind::Type(item)
        } else if let Ok(item) = input.try_parse() {
            TraitItemKind::Const(item)
        } else if let Ok(item) = input.try_parse() {
            TraitItemKind::Function(item)
        } else {
            return Err(Diagnostics::new_error_spanned(
                "Expected a trait item",
                input.span(),
            ));
        };
        Ok(Self { attributes, kind })
    }
}

impl Parse for TraitFunction {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let signature: FnSignature = input.parse()?;
        let body = if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Brace
        {
            TraitFunctionBody::Block(input.parse()?)
        } else {
            TraitFunctionBody::Semicolon(input.parse()?)
        };
        Ok(Self { signature, body })
    }
}

impl ToTokens for TraitItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.unsafety.to_tokens(tokens);
        self.auto_token.to_tokens(tokens);
        self.trait_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.generics.to_tokens(tokens);
        self.bounds.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
        self.items.to_tokens(tokens);
    }
}

impl ToTokens for TraitItemMember {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
        self.kind.to_tokens(tokens);
    }
}

impl ToTokens for TraitItemKind {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Type(item) => item.to_tokens(tokens),
            Self::Const(item) => item.to_tokens(tokens),
            Self::Function(item) => item.to_tokens(tokens),
        }
    }
}

impl ToTokens for TraitFunction {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.signature.to_tokens(tokens);
        self.body.to_tokens(tokens);
    }
}

impl ToTokens for TraitFunctionBody {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Block(block) => block.to_tokens(tokens),
            Self::Semicolon(semicolon) => semicolon.to_tokens(tokens),
        }
    }
}
