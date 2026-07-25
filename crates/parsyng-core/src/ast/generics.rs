//! Codegen-facing views over [`GenericParams`] that render it appropriately
//! for the `impl<...>` header versus the `Type<...>` position.
//!
//! Neither view is meant to be parsed — they only exist to be interpolated
//! with [`quote!`](crate::quote!), and are produced by
//! [`Struct::split_generics_for_impl`](crate::ast::item::struct::Struct::split_generics_for_impl)
//! or
//! [`DeriveInput::split_generics_for_impl`](crate::ast::item::DeriveInput::split_generics_for_impl).
//!
//! Reference: <https://doc.rust-lang.org/reference/items/generics.html>

use crate::{
    ToTokens,
    ast::item::{GenericParam, GenericParams},
    ast::tokens::Comma,
    proc_macro::Span,
};

/// Renders [`GenericParams`] for an `impl<...>` header: every parameter in
/// full, including its bounds, lifetimes emitted first.
///
/// # Example
///
/// Given `struct Foo<T: Clone>`, this renders `<T: Clone>` — suitable for
/// `impl #impl_generics Trait for Foo #ty_generics { ... }`. See
/// [`TypeGenerics`] for the matching `Foo<T>` (bare parameter names) form.
pub struct ImplGenerics<'a>(&'a GenericParams);
/// Renders [`GenericParams`] for a type position (e.g. `Foo<...>`): bare
/// parameter names/lifetimes only, no bounds or defaults.
///
/// # Example
///
/// Given `struct Foo<T: Clone>`, this renders `<T>`. See [`ImplGenerics`]
/// for the matching `impl<T: Clone>` (full bounds) form.
pub struct TypeGenerics<'a>(&'a GenericParams);

impl<'a> From<&'a GenericParams> for ImplGenerics<'a> {
    fn from(value: &'a GenericParams) -> Self {
        Self(value)
    }
}

impl<'a> From<&'a GenericParams> for TypeGenerics<'a> {
    fn from(value: &'a GenericParams) -> Self {
        Self(value)
    }
}

impl ToTokens for ImplGenerics<'_> {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.0.start_token.to_tokens(tokens);
        // generate lifetimes first
        self.0
            .generics
            .iter()
            .filter(|generic| matches!(generic, GenericParam::Lifetime(_)))
            .for_each(|generic| {
                generic.to_tokens(tokens);
                Comma::new(Span::call_site()).to_tokens(tokens);
            });
        // and then other generics
        self.0
            .generics
            .iter()
            .filter(|generic| !matches!(generic, GenericParam::Lifetime(_)))
            .for_each(|generic| {
                generic.to_tokens(tokens);
                Comma::new(Span::call_site()).to_tokens(tokens);
            });
        self.0.last_token.to_tokens(tokens);
    }
}

impl ToTokens for TypeGenerics<'_> {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.0.start_token.to_tokens(tokens);
        self.0.generics.iter().for_each(|generic| {
            match generic {
                GenericParam::Type(type_param) => type_param.ident.to_tokens(tokens),
                GenericParam::Lifetime(lifetime_param) => lifetime_param.to_tokens(tokens),
                GenericParam::Const(const_param) => const_param.ident.to_tokens(tokens),
            }
            Comma::new(Span::call_site()).to_tokens(tokens);
        });
        self.0.last_token.to_tokens(tokens);
    }
}
