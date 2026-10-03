//! `enum` items.

use crate::ToTokens;

use crate::ast::generics::{ImplGenerics, TypeGenerics};
use crate::{
    ast::{
        attributes::{Attribute, parse_outer_attributes},
        delimiter::{Braced, Parenthesized},
        item::{GenericParams, WhereClause},
        token_stream::TokenStreamUntilComma,
        tokens::{Colon, Comma, Enum, Eq},
        r#type::Type,
    },
    combinator::Punctuated,
    parse::Parse,
    proc_macro::{Delimiter, Ident},
};

/// An `enum` item, without its leading attributes/visibility (see
/// [`ItemEnum`](crate::ast::item::ItemEnum) for that): `enum Foo<T> where
/// ... { ... }`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/enumerations.html>
#[derive(Clone, Debug)]
pub struct EnumItem {
    enum_token: Enum,
    ident: Ident,
    generics: Option<GenericParams>,
    where_clause: Option<WhereClause>,
    variants: Braced<Punctuated<EnumVariant, Comma>>,
}

/// One variant of an [`EnumItem`], e.g. `Foo { a: i32 } = 1`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/enumerations.html>
#[derive(Clone, Debug)]
pub struct EnumVariant {
    attributes: Vec<Attribute>,
    ident: Ident,
    fields: EnumVariantFields,
    discriminant: Option<(Eq, TokenStreamUntilComma)>,
}

/// An [`EnumVariant`]'s fields: named (`{ a: A }`), tuple (`(A, B)`), or
/// unit (no fields).
///
/// Reference: <https://doc.rust-lang.org/reference/items/enumerations.html>
#[derive(Clone, Debug)]
pub enum EnumVariantFields {
    /// `{ a: A, b: B }`.
    Named(Braced<Punctuated<EnumField, Comma>>),
    /// `(A, B)`.
    Unnamed(Parenthesized<Punctuated<Type, Comma>>),
    /// No fields.
    Unit,
}

/// A named field inside an [`EnumVariantFields::Named`] variant.
///
/// Reference: <https://doc.rust-lang.org/reference/items/enumerations.html>
#[derive(Clone, Debug)]
pub struct EnumField {
    attributes: Vec<Attribute>,
    ident: Ident,
    colon: Colon,
    ty: Type,
}

impl EnumItem {
    /// This enum's name.
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        &self.ident
    }
    /// This enum's generic parameters, if any.
    #[must_use]
    pub const fn generic_parameters(&self) -> Option<&GenericParams> {
        self.generics.as_ref()
    }
    /// Mutable access to this enum's generic parameters, for adding trait
    /// bounds before re-emitting them (see
    /// [`TypeParamBounds::push`](crate::ast::item::TypeParamBounds::push)).
    pub const fn generic_parameters_mut(&mut self) -> Option<&mut GenericParams> {
        self.generics.as_mut()
    }
    /// This enum's `where` clause, if any.
    #[must_use]
    pub const fn where_clause(&self) -> Option<&WhereClause> {
        self.where_clause.as_ref()
    }
    /// Split this enum's generics into the `impl<...>`, `Type<...>` and
    /// `where ...` pieces needed to build a trait impl.
    #[must_use]
    pub fn split_generics_for_impl(
        &self,
    ) -> (
        Option<ImplGenerics<'_>>,
        Option<TypeGenerics<'_>>,
        Option<&WhereClause>,
    ) {
        (
            self.generic_parameters().map(Into::into),
            self.generic_parameters().map(Into::into),
            self.where_clause.as_ref(),
        )
    }
    /// This enum's variants.
    #[must_use]
    pub fn variants(&self) -> &Punctuated<EnumVariant, Comma> {
        &self.variants
    }
    /// Mutable access to this enum's variants.
    pub fn variants_mut(&mut self) -> &mut Punctuated<EnumVariant, Comma> {
        &mut self.variants
    }
}

impl EnumVariant {
    /// This variant's name.
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        &self.ident
    }
    /// This variant's outer attributes.
    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// This variant's fields.
    #[must_use]
    pub const fn fields(&self) -> &EnumVariantFields {
        &self.fields
    }
    /// The tokens of this variant's explicit discriminant (`= <expr>`), if
    /// any.
    #[must_use]
    pub fn discriminant(&self) -> Option<&TokenStreamUntilComma> {
        self.discriminant
            .as_ref()
            .map(|(_, discriminant)| discriminant)
    }
}

impl EnumField {
    /// This field's name.
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        &self.ident
    }
    /// This field's outer attributes.
    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// This field's type.
    #[must_use]
    pub const fn ty(&self) -> &Type {
        &self.ty
    }
}

impl Parse for EnumItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            enum_token: input.parse()?,
            ident: input.parse()?,
            generics: input.try_parse().ok(),
            where_clause: input.try_parse().ok(),
            variants: input.parse()?,
        })
    }
}

impl Parse for EnumVariant {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        let ident = input.parse()?;
        let fields = if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Brace
        {
            EnumVariantFields::Named(input.parse()?)
        } else if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Parenthesis
        {
            EnumVariantFields::Unnamed(input.parse()?)
        } else {
            EnumVariantFields::Unit
        };
        let discriminant = if let Ok(eq) = input.peek_parse::<Eq>() {
            Some((eq, input.parse()?))
        } else {
            None
        };
        Ok(Self {
            attributes,
            ident,
            fields,
            discriminant,
        })
    }
}

impl Parse for EnumField {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        Ok(Self {
            attributes,
            ident: input.parse()?,
            colon: input.parse()?,
            ty: input.parse()?,
        })
    }
}

impl ToTokens for EnumItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.enum_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.generics.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
        self.variants.to_tokens(tokens);
    }
}

impl ToTokens for EnumVariant {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.fields.to_tokens(tokens);
        self.discriminant.to_tokens(tokens);
    }
}

impl ToTokens for EnumVariantFields {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Named(fields) => fields.to_tokens(tokens),
            Self::Unnamed(fields) => fields.to_tokens(tokens),
            Self::Unit => {}
        }
    }
}

impl ToTokens for EnumField {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.ty.to_tokens(tokens);
    }
}
