//! Members of an [`Implementation`](crate::ast::item::implementation::Implementation)
//! block.

use crate::ToTokens;

use crate::{
    ast::{
        attributes::parse_outer_attributes,
        item::{
            ItemFunction, associated::TypeAlias, constant::ConstantItem,
            macro_item::MacroInvocationItem,
        },
    },
    error::Diagnostics,
    parse::Parse,
};

/// One member inside an `impl { ... }` block.
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html>
#[derive(Clone, Debug)]
pub struct ImplItem {
    attributes: Vec<crate::ast::attributes::Attribute>,
    kind: ImplItemKind,
}

/// An [`ImplItem`]'s kind: an associated type, associated const, method, or
/// a macro invocation in item position.
///
/// The method variant carries its own attributes and visibility, unlike
/// [`TraitItemKind::Function`](crate::ast::item::trait_item::TraitItemKind::Function).
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html>
#[derive(Clone, Debug)]
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
    Function(Box<ItemFunction>),
    /// A macro invocation.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
    Macro(MacroInvocationItem),
}

impl Parse for ImplItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
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
        Ok(Self { attributes, kind })
    }
}

impl ToTokens for ImplItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attributes.to_tokens(tokens);
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
