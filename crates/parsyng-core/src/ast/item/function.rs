//! Free function items.

use crate::ToTokens;

use crate::{
    ast::{delimiter::Braced, signature::FnSignature, tokens::Semicolon},
    parse::Parse,
    proc_macro::{Delimiter, TokenStream},
};

/// A free function item, without its leading attributes/visibility (see
/// [`ItemFunction`](crate::ast::item::ItemFunction) for that): `fn foo(...)
/// -> T { ... }`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/functions.html>
#[derive(Clone, Debug)]
pub struct FunctionItem {
    signature: FnSignature,
    body: FunctionBody,
}

/// A [`FunctionItem`]'s body: a `{ ... }` block (kept as a raw, unparsed
/// [`TokenStream`]) or a bare `;` (a bodiless declaration, as used in
/// `extern` blocks and trait method declarations).
///
/// Reference: <https://doc.rust-lang.org/reference/items/functions.html>
#[derive(Clone, Debug)]
pub enum FunctionBody {
    /// `{ ... }`.
    Block(Braced<TokenStream>),
    /// A bare `;` (no body).
    Semicolon(Semicolon),
}

impl FunctionItem {
    /// This function's signature.
    #[must_use]
    pub const fn signature(&self) -> &FnSignature {
        &self.signature
    }
}

impl Parse for FunctionItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let signature = input.parse()?;
        let body = if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Brace
        {
            FunctionBody::Block(input.parse()?)
        } else {
            FunctionBody::Semicolon(input.parse()?)
        };
        Ok(Self { signature, body })
    }
}

impl ToTokens for FunctionItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.signature.to_tokens(tokens);
        self.body.to_tokens(tokens);
    }
}

impl ToTokens for FunctionBody {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Block(block) => block.to_tokens(tokens),
            Self::Semicolon(semicolon) => semicolon.to_tokens(tokens),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate as parsyng;
    use parsyng_quote_macros::quote;

    use super::*;
    use crate::ast::tests::check;

    #[test]
    fn test_impl_arguments() {
        check::<FunctionItem>(quote! {
            fn with<T>(&self, f: impl FnOnce() -> T) -> T {
                CURRENT.with(|local_data| {
                    let _guard = local_data.enter(self.context.clone());
                    f()
                })
            }
        });
    }
}
