//! Members of an [`Implementation`](crate::ast::item::implementation::Implementation)
//! block.

use crate::ToTokens;

use crate::{
    ast::{
        attributes::{Attribute, parse_outer_attributes},
        item::{
            associated::TypeAlias, constant::ConstantItem, function::FunctionItem,
            macro_item::MacroInvocationItem,
        },
        visibility::Visibility,
    },
    error::Diagnostics,
    parse::Parse,
    proc_macro::Ident,
};

/// One member inside an `impl { ... }` block, with its attributes,
/// visibility and (nightly specialization) `default` marker.
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct ImplItem {
    attributes: Vec<Attribute>,
    visibility: Visibility,
    defaultness: Option<Ident>,
    kind: ImplItemKind,
}

impl ImplItem {
    /// This member's outer attributes.
    #[must_use]
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// This member's visibility.
    #[must_use]
    pub const fn visibility(&self) -> &Visibility {
        &self.visibility
    }
    /// What this member is.
    #[must_use]
    pub const fn kind(&self) -> &ImplItemKind {
        &self.kind
    }
}

/// An [`ImplItem`]'s kind: an associated type, associated const, method, or
/// a macro invocation in item position.
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum ImplItemKind {
    /// An associated type.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-types>
    Type(Box<TypeAlias>),
    /// An associated const.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-constants>
    Const(Box<ConstantItem>),
    /// A method.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-functions-and-methods>
    Function(Box<FunctionItem>),
    /// A macro invocation.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
    Macro(MacroInvocationItem),
}

impl Parse for ImplItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        let visibility = input.parse()?;
        // `default fn`, but not a `default` function or macro.
        let defaultness = if input.peek_ident_str() == Some("default")
            && input.nth_punct_char(1).is_none()
            && input.nth_delimiter(1).is_none()
        {
            input.ident()
        } else {
            None
        };
        let kind = if let Ok(item) = input.try_parse() {
            ImplItemKind::Type(item)
        } else if let Ok(item) = input.try_parse() {
            ImplItemKind::Const(item)
        } else if let Ok(item) = input.try_parse() {
            ImplItemKind::Function(item)
        } else if let Ok(item) = input.try_parse() {
            ImplItemKind::Macro(item)
        } else {
            return Err(Diagnostics::new_error_spanned(
                "Expected an impl item",
                input.span(),
            ));
        };
        Ok(Self {
            attributes,
            visibility,
            defaultness,
            kind,
        })
    }
}

impl ToTokens for ImplItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
        self.visibility.to_tokens(tokens);
        self.defaultness.to_tokens(tokens);
        self.kind.to_tokens(tokens);
    }
}

impl ToTokens for ImplItemKind {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Type(item) => item.to_tokens(tokens),
            Self::Const(item) => item.to_tokens(tokens),
            Self::Function(item) => item.to_tokens(tokens),
            Self::Macro(item) => item.to_tokens(tokens),
        }
    }
}
