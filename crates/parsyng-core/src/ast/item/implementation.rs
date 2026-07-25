//! `impl` blocks.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Braced,
        item::{GenericParams, WhereClause, impl_item::ImplItem},
        tokens::{For, Impl, Not, Unsafe},
        r#type::{Type, TypePath},
    },
    parse::Parse,
};

/// An `impl` block, without its leading attributes/visibility (see
/// [`ItemImpl`](crate::ast::item::ItemImpl) for that): `unsafe impl<T>
/// !Trait for Foo<T> where ... { ... }`.
///
/// `trait_impl` is `Some((negation, trait_path, for_token))` for a trait
/// impl (the leading `Option<Not>` covers negative impls like `impl !Send
/// for Foo`) and `None` for an inherent impl.
///
/// Reference: <https://doc.rust-lang.org/reference/items/implementations.html>
#[derive(Clone, Debug)]
pub struct Implementation {
    unsafety: Option<Unsafe>,
    impl_token: Impl,
    generic_parameters: Option<GenericParams>,
    trait_impl: Option<(Option<Not>, TypePath, For)>,
    ty: Type,
    where_clause: Option<WhereClause>,
    associated_items: Braced<Vec<ImplItem>>,
}

impl Parse for Implementation {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let unsafety = input.try_parse().ok();
        let impl_token = input.parse()?;
        let generic_parameters = input.try_parse().ok();
        // trait_impl can be: `!Trait for` or `TraitPath for` or absent
        let trait_impl = if let Ok(not_token) = input.try_parse::<Not>() {
            let path: TypePath = input.parse()?;
            let for_token: For = input.parse()?;
            Some((Some(not_token), path, for_token))
        } else {
            input
                .try_advance(|input| Ok((None, input.parse::<TypePath>()?, input.parse::<For>()?)))
                .ok()
        };
        Ok(Self {
            unsafety,
            impl_token,
            generic_parameters,
            trait_impl,
            ty: input.parse()?,
            where_clause: input.try_parse().ok(),
            associated_items: input.parse()?,
        })
    }
}

impl ToTokens for Implementation {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.unsafety.to_tokens(tokens);
        self.impl_token.to_tokens(tokens);
        self.generic_parameters.to_tokens(tokens);
        self.trait_impl.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
        self.associated_items.to_tokens(tokens);
    }
}

#[cfg(test)]
mod tests {
    use crate as parsyng;
    use parsyng_quote_macros::quote;

    use super::*;
    use crate::ast::tests::check;

    #[test]
    fn test_implementation() {
        check::<Implementation>(quote! {
            impl<T> Sender<T> {
                fn weak_count() {
                }
            }
        });
    }
}
