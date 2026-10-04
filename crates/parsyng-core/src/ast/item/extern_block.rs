//! `extern` blocks.

use crate::ToTokens;

use crate::{
    ast::{
        attributes::{Attribute, parse_outer_attributes},
        delimiter::Braced,
        item::{
            ItemList, associated::TypeAlias, macro_item::MacroInvocationItem,
            static_item::StaticItem,
        },
        signature::FnSignature,
        tokens::{Extern, Semicolon, Unsafe},
        visibility::Visibility,
    },
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::{Ident, Literal},
};

/// An `extern` block, without its leading attributes/visibility (see
/// [`ItemExternBlock`](crate::ast::item::ItemExternBlock) for that):
/// `unsafe extern "C" { ... }`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/external-blocks.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct ExternBlockItem {
    unsafe_token: Option<Unsafe>,
    extern_token: Extern,
    abi: Option<Literal>,
    items: Braced<ItemList<ExternItem>>,
}

impl ExternBlockItem {
    /// The block's items.
    #[must_use]
    pub fn items(&self) -> &[ExternItem] {
        self.items.inner_ref().items()
    }
}

/// One item inside an `extern` block, with its attributes and visibility.
///
/// Reference: <https://doc.rust-lang.org/reference/items/external-blocks.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct ExternItem {
    attributes: Vec<Attribute>,
    visibility: Visibility,
    kind: ExternItemKind,
}

impl ExternItem {
    /// This item's outer attributes.
    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// This item's visibility.
    #[must_use]
    pub const fn visibility(&self) -> &Visibility {
        &self.visibility
    }
    /// What this item is.
    #[must_use]
    pub const fn kind(&self) -> &ExternItemKind {
        &self.kind
    }
}

/// An [`ExternItem`]'s kind.
///
/// Reference: <https://doc.rust-lang.org/reference/items/external-blocks.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum ExternItemKind {
    /// A function declaration: `safe fn f(x: i32) -> i32;` (an `unsafe`
    /// qualifier is part of the signature).
    Function(Option<Ident>, Box<FnSignature>, Semicolon),
    /// A static: `safe static mut X: i32;`.
    Static(Option<Safety>, Box<StaticItem>),
    /// An extern type (nightly): `type Opaque;`.
    Type(Box<TypeAlias>),
    /// A macro invocation.
    Macro(MacroInvocationItem),
}

/// The `safe`/`unsafe` qualifier of an item in an `unsafe extern` block.
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum Safety {
    /// `safe` (a weak keyword).
    Safe(Ident),
    /// `unsafe`.
    Unsafe(Unsafe),
}

/// A `safe` weak keyword followed by `next`.
fn parse_safe(input: &mut ParseBuffer, next: &str) -> Option<Ident> {
    if input.peek_ident_str() == Some("safe") && input.nth_ident_str(1) == Some(next) {
        input.ident()
    } else {
        None
    }
}

impl Parse for ExternBlockItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            unsafe_token: input.try_parse().ok(),
            extern_token: input.parse()?,
            abi: input.try_parse().ok(),
            items: input.parse()?,
        })
    }
}

impl Parse for ExternItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        let visibility = input.parse()?;
        let kind = if let Some(safe) = parse_safe(input, "static") {
            ExternItemKind::Static(Some(Safety::Safe(safe)), input.parse()?)
        } else if input.peek_ident_str() == Some("unsafe")
            && input.nth_ident_str(1) == Some("static")
        {
            ExternItemKind::Static(Some(Safety::Unsafe(input.parse()?)), input.parse()?)
        } else if input.peek_ident_str() == Some("static") {
            ExternItemKind::Static(None, input.parse()?)
        } else if input.peek_ident_str() == Some("type") {
            ExternItemKind::Type(input.parse()?)
        } else if let Ok((safe, signature, semi)) =
            input.try_advance(|input| Ok((parse_safe(input, "fn"), input.parse()?, input.parse()?)))
        {
            ExternItemKind::Function(safe, signature, semi)
        } else if let Ok(invocation) = input.try_parse() {
            ExternItemKind::Macro(invocation)
        } else {
            return Err(Diagnostics::new_error_spanned(
                "Expected an extern item",
                input.span(),
            ));
        };
        Ok(Self {
            attributes,
            visibility,
            kind,
        })
    }
}

impl ToTokens for ExternBlockItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.unsafe_token.to_tokens(tokens);
        self.extern_token.to_tokens(tokens);
        self.abi.to_tokens(tokens);
        self.items.to_tokens(tokens);
    }
}

impl ToTokens for ExternItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
        self.visibility.to_tokens(tokens);
        self.kind.to_tokens(tokens);
    }
}

impl ToTokens for ExternItemKind {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Function(safe, signature, semi) => {
                safe.to_tokens(tokens);
                signature.to_tokens(tokens);
                semi.to_tokens(tokens);
            }
            Self::Static(safety, item) => {
                safety.to_tokens(tokens);
                item.to_tokens(tokens);
            }
            Self::Type(item) => item.to_tokens(tokens),
            Self::Macro(item) => item.to_tokens(tokens),
        }
    }
}

impl ToTokens for Safety {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Safe(safe) => safe.to_tokens(tokens),
            Self::Unsafe(unsafe_token) => unsafe_token.to_tokens(tokens),
        }
    }
}
