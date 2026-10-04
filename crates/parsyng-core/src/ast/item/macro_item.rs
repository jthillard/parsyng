//! `macro_rules!`/`macro` declarations, and macro invocations used in item
//! position.

use crate::ToTokens;

use crate::{
    ast::{
        path::SimplePath,
        tokens::{Macro, Not, Semicolon},
    },
    error::Diagnostics,
    parse::Parse,
    proc_macro::{Delimiter, Group, Ident, Span},
};

/// A `macro_rules! name { ... }` declarative macro definition. The body is
/// kept as a raw, opaque [`Group`]; a `(...)` or `[...]` body is followed
/// by a `;`.
///
/// Does not include leading attributes/visibility — see
/// [`ItemMacroRules`](crate::ast::item::ItemMacroRules) for that.
///
/// Reference: <https://doc.rust-lang.org/reference/macros-by-example.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct MacroRulesItem {
    macro_rules_ident: Ident,
    bang: Not,
    name: Ident,
    body: Group,
    semi: Option<Semicolon>,
}

/// A Rust-2.0-style `macro name { ... }` declarative macro definition. The
/// body is kept as a raw, opaque [`Group`].
///
/// Does not include leading attributes/visibility — see
/// [`ItemMacro`](crate::ast::item::ItemMacro) for that.
///
/// Reference: <https://doc.rust-lang.org/reference/macros-by-example.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct MacroItem {
    macro_token: Macro,
    name: Ident,
    body: Group,
}

/// A macro invocation used in item position, e.g. `my_macro!{ ... }` or
/// `my_macro!(...);`.
///
/// Reused beyond item position too — as
/// [`Type::MacroInvocation`](crate::ast::type::Type::MacroInvocation) and
/// [`ImplItemKind::Macro`](crate::ast::item::impl_item::ImplItemKind::Macro).
///
/// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct MacroInvocationItem {
    path: SimplePath,
    bang: Not,
    body: Group,
    semi: Option<Semicolon>,
}

impl MacroInvocationItem {
    /// The span of this invocation's path.
    #[must_use]
    pub fn span(&self) -> Span {
        self.path.span()
    }
}

impl Parse for MacroRulesItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let Some(macro_rules_ident) = input.ident_str_and(|text| text == "macro_rules") else {
            return Err(Diagnostics::new_error_spanned(
                "Expected `macro_rules`",
                input.span(),
            ));
        };
        let bang = input.parse()?;
        let name = input.parse()?;
        let body: Group = input.parse()?;
        let semi = if body.delimiter() == Delimiter::Brace {
            None
        } else {
            Some(input.parse()?)
        };
        Ok(Self {
            macro_rules_ident,
            bang,
            name,
            body,
            semi,
        })
    }
}

impl Parse for MacroItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            macro_token: input.parse()?,
            name: input.parse()?,
            body: input.parse()?,
        })
    }
}

impl MacroInvocationItem {
    /// Parse an invocation without a trailing `;`, as in type position,
    /// where a following `;` belongs to the enclosing item.
    pub(crate) fn parse_without_semicolon(
        input: &mut crate::parse::ParseBuffer,
    ) -> crate::error::Result<Self> {
        // A path can't start with a strict keyword: `if !(a).b {}` is an
        // `if`, not an `if!(a)` invocation.
        if input.peek_keyword().is_some()
            && !matches!(
                input.peek_ident_str(),
                Some("crate" | "self" | "Self" | "super" | "auto" | "default" | "raw" | "union")
            )
        {
            return Err(Diagnostics::new_error_spanned(
                "Expected a macro path",
                input.span(),
            ));
        }
        Ok(Self {
            path: input.parse()?,
            bang: input.parse()?,
            body: input.parse()?,
            semi: None,
        })
    }
}

impl Parse for MacroInvocationItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let mut invocation = Self::parse_without_semicolon(input)?;
        invocation.semi = input.try_parse().ok();
        Ok(invocation)
    }
}

impl ToTokens for MacroRulesItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.macro_rules_ident.to_tokens(tokens);
        self.bang.to_tokens(tokens);
        self.name.to_tokens(tokens);
        tokens.extend(Some(self.body.clone()));
        self.semi.to_tokens(tokens);
    }
}

impl ToTokens for MacroItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.macro_token.to_tokens(tokens);
        self.name.to_tokens(tokens);
        tokens.extend(Some(self.body.clone()));
    }
}

impl ToTokens for MacroInvocationItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path.to_tokens(tokens);
        self.bang.to_tokens(tokens);
        tokens.extend(Some(self.body.clone()));
        self.semi.to_tokens(tokens);
    }
}
