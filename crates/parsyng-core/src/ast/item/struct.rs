//! `struct` items: [`Struct`] (wrapped, with attributes and visibility, as
//! [`ItemStruct`](crate::ast::item::ItemStruct)), its field list
//! ([`StructFields`]), and named/tuple field types.

use crate::ToTokens;

use crate::ast::attributes::Attribute;
use crate::ast::generics::{ImplGenerics, TypeGenerics};
use crate::{
    ast::{
        attributes::parse_outer_attributes,
        delimiter::{Braced, Parenthesized},
        item::{GenericParams, WhereClause},
        tokens::{Colon, Comma, Semicolon, StructKeyword},
        r#type::Type,
        visibility::Visibility,
    },
    combinator::Punctuated,
    parse::Parse,
    proc_macro::{Delimiter, Ident, Span},
};

/// A `struct` item, without its leading attributes/visibility (see
/// [`ItemStruct`](crate::ast::item::ItemStruct) for that): `struct
/// Foo<T> where ... { a: A }`, a tuple struct, or a unit struct.
///
/// Reference: <https://doc.rust-lang.org/reference/items/structs.html>
#[derive(Clone, Debug)]
pub struct Struct {
    #[allow(clippy::struct_field_names)]
    struct_token: StructKeyword,
    ident: Ident,
    /// This struct's generic parameters, if any.
    pub generic_parameters: Option<GenericParams>,
    where_clause: Option<WhereClause>,
    /// This struct's fields.
    pub fields: StructFields,
    semicolon: Option<Semicolon>,
}

impl Struct {
    /// This struct's name.
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        &self.ident
    }
    /// This struct's generic parameters, if any.
    #[must_use]
    pub const fn generic_parameters(&self) -> Option<&GenericParams> {
        self.generic_parameters.as_ref()
    }
    /// Mutable access to this struct's generic parameters, for adding trait
    /// bounds before re-emitting them (see
    /// [`TypeParamBounds::push`](crate::ast::item::TypeParamBounds::push)).
    pub const fn generic_parameters_mut(&mut self) -> Option<&mut GenericParams> {
        self.generic_parameters.as_mut()
    }
    /// Split this struct's generics into the `impl<...>`, `Type<...>` and
    /// `where ...` pieces needed to build a trait impl.
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
}


impl Parse for Struct {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let struct_token = input.parse()?;
        let struct_ident = input.parse()?;
        let generic_parameters = input.try_parse().ok();
        let where_clause = input.try_parse().ok();
        let (fields, semicolon) = if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Brace
        {
            (StructFields::Named(input.parse()?), None)
        } else if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Parenthesis
        {
            (StructFields::Unnamed(input.parse()?), Some(input.parse()?))
        } else {
            (StructFields::Unit, Some(input.parse()?))
        };

        Ok(Self {
            struct_token,
            ident: struct_ident,
            generic_parameters,
            where_clause,
            fields,
            semicolon,
        })
    }
}

impl ToTokens for Struct {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.struct_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.generic_parameters.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
        self.fields.to_tokens(tokens);
        self.semicolon.to_tokens(tokens);
    }
}

/// A named field in a [`StructFields::Named`] list: `pub a: i32`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/structs.html>
#[derive(Clone, Debug)]
pub struct StructField {
    attributes: Vec<Attribute>,
    visibility: Visibility,
    /// This field's name.
    pub ident: Ident,
    colon_token: Colon,
    ty: Type,
}

/// A struct's field list: named (`{ a: A, b: B }`), tuple (`(A, B)`), or
/// unit (no fields at all).
///
/// Reference: <https://doc.rust-lang.org/reference/items/structs.html>
#[derive(Clone, Debug)]
pub enum StructFields {
    /// `{ a: A, b: B }`.
    Named(Box<Braced<Punctuated<StructField, Comma>>>),
    /// `(A, B)`.
    Unnamed(Box<Parenthesized<Punctuated<TupleField, Comma>>>),
    /// No fields.
    Unit,
}

/// One positional field in a [`StructFields::Unnamed`] tuple struct, e.g.
/// `pub(crate) i32`.
///
/// Unlike [`StructField`], it has no `ident`, only a visibility-less type
/// (visibility on tuple fields isn't parsed here).
///
/// Reference: <https://doc.rust-lang.org/reference/items/structs.html>
#[derive(Clone, Debug)]
pub struct TupleField {
    attributes: Vec<crate::ast::attributes::Attribute>,
    ty: Type,
}

impl StructField {
    /// This field's span (its name).
    #[must_use]
    pub fn span(&self) -> Span {
        self.ident.span()
    }
    /// This field's name.
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        &self.ident
    }
}

impl TupleField {
    /// This field's span (its type).
    #[must_use]
    pub fn span(&self) -> Span {
        self.ty.span()
    }
}

impl Parse for StructField {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        Ok(Self {
            attributes,
            visibility: input.parse()?,
            ident: input.parse()?,
            colon_token: input.parse()?,
            ty: input.parse()?,
        })
    }
}

impl Parse for TupleField {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            attributes: parse_outer_attributes(input),
            ty: input.parse()?,
        })
    }
}

impl ToTokens for StructField {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
        self.visibility.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.colon_token.to_tokens(tokens);
        self.ty.to_tokens(tokens);
    }
}

impl ToTokens for StructFields {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Named(fields) => fields.to_tokens(tokens),
            Self::Unnamed(fields) => fields.to_tokens(tokens),
            Self::Unit => {}
        }
    }
}

impl ToTokens for TupleField {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
        self.ty.to_tokens(tokens);
    }
}

#[cfg(test)]
mod tests {
    use crate as parsyng;
    use parsyng_quote_macros::quote;

    use super::*;
    use crate::ast::tests::check;

    #[test]
    fn test_doc_attributes() {
        check::<Struct>(quote! {
            struct Receiver<T> {
                /// State shared with all receivers and senders.
                shared: Arc<Shared<T>>,

                /// Next position to read from
                next: u64,
            }
        });
    }
}
