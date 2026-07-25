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
    proc_macro::{Group, Ident},
};

/// A `macro_rules! name { ... }` declarative macro definition. The body is
/// kept as a raw, opaque [`Group`].
///
/// Does not include leading attributes/visibility — see
/// [`ItemMacroRules`](crate::ast::item::ItemMacroRules) for that.
///
/// Reference: <https://doc.rust-lang.org/reference/macros-by-example.html>
#[derive(Clone, Debug)]
pub struct MacroRulesItem {
    macro_rules_ident: Ident,
    bang: Not,
    name: Ident,
    body: Group,
}

/// A Rust-2.0-style `macro name { ... }` declarative macro definition. The
/// body is kept as a raw, opaque [`Group`].
///
/// Does not include leading attributes/visibility — see
/// [`ItemMacro`](crate::ast::item::ItemMacro) for that.
///
/// Reference: <https://doc.rust-lang.org/reference/macros-by-example.html>
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct MacroInvocationItem {
    path: SimplePath,
    bang: Not,
    body: Group,
    semi: Option<Semicolon>,
}

impl Parse for MacroRulesItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let macro_rules_ident: Ident = input.parse()?;
        #[allow(clippy::cmp_owned)]
        if macro_rules_ident.to_string() != "macro_rules" {
            return Err(Diagnostics::new_error_spanned(
                "Expected `macro_rules`",
                macro_rules_ident.span(),
            ));
        }
        Ok(Self {
            macro_rules_ident,
            bang: input.parse()?,
            name: input.parse()?,
            body: input.parse()?,
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

impl Parse for MacroInvocationItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let path: SimplePath = input.parse()?;
        let bang: Not = input.parse()?;
        let body: Group = input.parse()?;
        let semi = input.try_parse().ok();
        Ok(Self {
            path,
            bang,
            body,
            semi,
        })
    }
}

impl ToTokens for MacroRulesItem {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.macro_rules_ident.to_tokens(tokens);
        self.bang.to_tokens(tokens);
        self.name.to_tokens(tokens);
        tokens.extend(Some(self.body.clone()));
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
