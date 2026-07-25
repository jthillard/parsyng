//! Item visibility: `pub`, `pub(crate)`, `pub(self)`, `pub(in path)`, or
//! private (no keyword at all).

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Parenthesized,
        path::SimplePath,
        tokens::{Crate, In, Pub, SelfValue},
    },
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::Delimiter,
};

/// An item's visibility qualifier.
///
/// Used pervasively as the `visibility` field of
/// [`VisItem<T>`](crate::ast::item::VisItem) and of a named struct field.
/// Parsing never fails: the absence of a `pub` keyword is the valid
/// [`Private`](Self::Private) variant, not an error.
///
/// Reference: <https://doc.rust-lang.org/reference/visibility-and-privacy.html>
#[derive(Clone, Debug)]
pub enum Visibility {
    /// `pub`.
    Public(Pub),
    /// `pub(crate)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/visibility-and-privacy.html#pubin-path-pubcrate-pubsuper-and-pubself>
    Crate(Pub, Parenthesized<Crate>),
    /// `pub(self)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/visibility-and-privacy.html#pubin-path-pubcrate-pubsuper-and-pubself>
    SelfVis(Pub, Parenthesized<SelfValue>),
    /// `pub(in path::to::mod)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/visibility-and-privacy.html#pubin-path-pubcrate-pubsuper-and-pubself>
    PubIn(Pub, Parenthesized<(In, SimplePath)>),
    /// No visibility keyword at all (private to the containing module).
    Private,
}

impl Parse for Visibility {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(pub_token) = input.peek_parse::<Pub>() {
            if let Some(group) = input.peek_group()
                && group.delimiter() == Delimiter::Parenthesis
            {
                let mut group_input = ParseBuffer::new(group.stream());
                if let Ok(crate_token) = group_input.peek_parse::<Crate>() {
                    Ok(Self::Crate(
                        pub_token,
                        Parenthesized::new(input.group().unwrap(), crate_token),
                    ))
                } else if let Ok(self_token) = group_input.peek_parse::<SelfValue>() {
                    Ok(Self::SelfVis(
                        pub_token,
                        Parenthesized::new(input.group().unwrap(), self_token),
                    ))
                } else if let Ok(in_token) = group_input.peek_parse::<In>() {
                    let path = group_input.parse()?;
                    Ok(Self::PubIn(
                        pub_token,
                        Parenthesized::new(input.group().unwrap(), (in_token, path)),
                    ))
                } else {
                    Err(Diagnostics::new_error_spanned(
                        "Expected `in`, `crate` or `self`",
                        input.span(),
                    ))
                }
            } else {
                Ok(Self::Public(pub_token))
            }
        } else {
            Ok(Self::Private)
        }
    }
}
impl ToTokens for Visibility {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Public(pub_keyword) => pub_keyword.to_tokens(tokens),
            Self::Crate(rust_keyword, parenthesized) => {
                rust_keyword.to_tokens(tokens);
                parenthesized.to_tokens(tokens);
            }
            Self::SelfVis(rust_keyword, parenthesized) => {
                rust_keyword.to_tokens(tokens);
                parenthesized.to_tokens(tokens);
            }
            Self::PubIn(rust_keyword, parenthesized) => {
                rust_keyword.to_tokens(tokens);
                parenthesized.to_tokens(tokens);
            }
            Self::Private => {}
        }
    }
}
