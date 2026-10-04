//! `union` items.

use crate::ToTokens;

use crate::ast::generics::{ImplGenerics, TypeGenerics};
use crate::{
    ast::{
        delimiter::Braced,
        item::{GenericParams, WhereClause, r#struct::StructField},
        tokens::Comma,
    },
    combinator::Punctuated,
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::Ident,
};

/// A `union` item, without its leading attributes/visibility (see
/// [`ItemUnion`](crate::ast::item::ItemUnion) for that): `union Foo<T>
/// where ... { a: A, b: B }`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/unions.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct UnionItem {
    /// `union` is a weak keyword, hence an [`Ident`].
    union_token: Ident,
    ident: Ident,
    generic_parameters: Option<GenericParams>,
    where_clause: Option<WhereClause>,
    fields: Braced<Punctuated<StructField, Comma>>,
}

impl UnionItem {
    /// This union's name.
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        &self.ident
    }
    /// This union's generic parameters, if any.
    #[must_use]
    pub const fn generic_parameters(&self) -> Option<&GenericParams> {
        self.generic_parameters.as_ref()
    }
    /// Mutable access to this union's generic parameters.
    pub const fn generic_parameters_mut(&mut self) -> Option<&mut GenericParams> {
        self.generic_parameters.as_mut()
    }
    /// This union's `where` clause, if any.
    #[must_use]
    pub const fn where_clause(&self) -> Option<&WhereClause> {
        self.where_clause.as_ref()
    }
    /// This union's fields.
    #[must_use]
    pub const fn fields(&self) -> &Punctuated<StructField, Comma> {
        self.fields.inner_ref()
    }
    /// Split this union's generics into the `impl<...>`, `Type<...>` and
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
}

impl Parse for UnionItem {
    fn parse(input: &mut ParseBuffer) -> crate::error::Result<Self> {
        // `union` is only a keyword when it's followed by the union's name.
        let Some(union_token) = input
            .nth_ident_str(1)
            .is_some()
            .then(|| input.ident_str_and(|text| text == "union"))
            .flatten()
        else {
            return Err(Diagnostics::new_error_spanned(
                "Expected `union`",
                input.span(),
            ));
        };
        Ok(Self {
            union_token,
            ident: input.parse()?,
            generic_parameters: input.try_parse().ok(),
            where_clause: input.try_parse().ok(),
            fields: input.parse()?,
        })
    }
}

impl ToTokens for UnionItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.union_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.generic_parameters.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
        self.fields.to_tokens(tokens);
    }
}
