//! Item visibility: `pub`, `pub(crate)`, `pub(self)`, `pub(super)`,
//! `pub(in path)`, or
//! private (no keyword at all).

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Parenthesized,
        path::SimplePath,
        tokens::{Crate, In, Pub, SelfValue, Super},
    },
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
    /// `pub(super)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/visibility-and-privacy.html#pubin-path-pubcrate-pubsuper-and-pubself>
    Super(Pub, Parenthesized<Super>),
    /// `pub(in path::to::mod)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/visibility-and-privacy.html#pubin-path-pubcrate-pubsuper-and-pubself>
    PubIn(Pub, Parenthesized<(In, SimplePath)>),
    /// No visibility keyword at all (private to the containing module).
    Private,
}

impl Parse for Visibility {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let Ok(pub_token) = input.peek_parse::<Pub>() else {
            return Ok(Self::Private);
        };
        let Some(group) = input.peek_group() else {
            return Ok(Self::Public(pub_token));
        };
        if group.delimiter() != Delimiter::Parenthesis {
            return Ok(Self::Public(pub_token));
        }
        // `pub(crate)`, `pub(self)`, `pub(super)` or `pub(in path)`. Any
        // other parenthesized group is not part of the visibility, e.g. the
        // tuple type in `struct S(pub (u8, u8));`.
        let mut group_input = ParseBuffer::new(group.stream());
        let visibility = if let Ok(crate_token) = group_input.peek_parse::<Crate>() {
            Restricted::Crate(crate_token)
        } else if let Ok(self_token) = group_input.peek_parse::<SelfValue>() {
            Restricted::SelfVis(self_token)
        } else if let Ok(super_token) = group_input.peek_parse::<Super>() {
            Restricted::Super(super_token)
        } else if let Ok(in_token) = group_input.peek_parse::<In>() {
            Restricted::PubIn(in_token, group_input.parse()?)
        } else {
            return Ok(Self::Public(pub_token));
        };
        if !group_input.is_empty() {
            return Ok(Self::Public(pub_token));
        }
        let Some(group) = input.group() else {
            unreachable!("a group was just peeked")
        };
        Ok(match visibility {
            Restricted::Crate(token) => Self::Crate(pub_token, Parenthesized::new(group, token)),
            Restricted::SelfVis(token) => {
                Self::SelfVis(pub_token, Parenthesized::new(group, token))
            }
            Restricted::Super(token) => Self::Super(pub_token, Parenthesized::new(group, token)),
            Restricted::PubIn(token, path) => {
                Self::PubIn(pub_token, Parenthesized::new(group, (token, path)))
            }
        })
    }
}

/// The contents of a `pub(...)` restriction.
enum Restricted {
    Crate(Crate),
    SelfVis(SelfValue),
    Super(Super),
    PubIn(In, SimplePath),
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
            Self::Super(rust_keyword, parenthesized) => {
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
