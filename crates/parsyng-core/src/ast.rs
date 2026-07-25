//! A tree of Rust syntax types, each implementing
//! [`Parse`](crate::parse::Parse) to build itself from a
//! [`ParseBuffer`](crate::parse::ParseBuffer) and
//! [`ToTokens`](crate::ToTokens) to turn itself back into tokens.
//!
//! There is no umbrella `Item`/`Expr`/`Type` prelude: every type lives at its
//! full path (`ast::item::r#struct::Struct`, `ast::expression::IfExpression`,
//! ...). The most commonly needed entry points are re-exported as `pub type`
//! aliases from [`item`](crate::ast::item) —
//! [`item::ItemStruct`](crate::ast::item::ItemStruct),
//! [`item::ItemEnum`](crate::ast::item::ItemEnum),
//! [`item::ItemFunction`](crate::ast::item::ItemFunction), etc. — and
//! [`item::DeriveInput`](crate::ast::item::DeriveInput), used as the input type of `#[derive(...)]` macros.
//!
//! # Coverage
//!
//! `parsyng`'s grammar coverage is intentionally narrower than `syn`'s —
//! wide enough to write typical derive/attribute macros over items, but a
//! few corners are still unimplemented (parsing them panics with `todo!()`
//! rather than returning a graceful [`Parse`](crate::parse::Parse) error):
//!
//! - [`literal::Literal`](crate::ast::literal::Literal) only parses numeric
//!   literals; string, char and byte(-string) literals are not yet
//!   implemented.
//! - [`pattern::Pattern`](crate::ast::pattern::Pattern) only covers
//!   identifier, wildcard, tuple and reference patterns — no literal, path,
//!   struct, slice or `|`-patterns.
//! - [`item::DeriveInput`](crate::ast::item::DeriveInput)'s generics/`ident`
//!   accessors only handle the `Struct` variant; the `Enum` variant is
//!   unimplemented.
//! - [`signature::FnParam::ident`](crate::ast::signature::FnParam::ident),
//!   [`signature::FnParam::mutability`](crate::ast::signature::FnParam::mutability)
//!   panic if called on a `SelfParam`.
//! - [`type::Type::span`](crate::ast::type::Type::span) only handles the
//!   `Never` (`!`) variant.
//!
//! # Module map
//!
//! | Module | Contents |
//! | --- | --- |
//! | [`attributes`](crate::ast::attributes) | `#[...]` / `#![...]` attributes |
//! | [`crate_source`](crate::ast::crate_source) | A whole source file ([`crate_source::Crate`](crate::ast::crate_source::Crate)) |
//! | [`delimiter`](crate::ast::delimiter) | Generic `[T]`/`{T}`/`(T)` wrappers ([`delimiter::Bracketed`](crate::ast::delimiter::Bracketed), [`delimiter::Braced`](crate::ast::delimiter::Braced), [`delimiter::Parenthesized`](crate::ast::delimiter::Parenthesized)) |
//! | [`expression`](crate::ast::expression) | Expressions ([`expression::Expression`](crate::ast::expression::Expression) and ~20 concrete kinds) |
//! | [`generics`](crate::ast::generics) | `impl`/type-position generics views for codegen ([`generics::ImplGenerics`](crate::ast::generics::ImplGenerics), [`generics::TypeGenerics`](crate::ast::generics::TypeGenerics)) |
//! | [`item`](crate::ast::item) | Top-level items ([`item::Item`](crate::ast::item::Item)), generics, where-clauses, and the per-kind submodules below |
//! | [`literal`](crate::ast::literal) | Numeric literals |
//! | [`path`](crate::ast::path) | Paths and generic arguments ([`path::SimplePath`](crate::ast::path::SimplePath), [`path::GenericArgs`](crate::ast::path::GenericArgs)) |
//! | [`pattern`](crate::ast::pattern) | Patterns ([`pattern::Pattern`](crate::ast::pattern::Pattern)) |
//! | [`signature`](crate::ast::signature) | Function signatures ([`signature::FnSignature`](crate::ast::signature::FnSignature)) |
//! | [`statements`](crate::ast::statements) | Block statements ([`statements::Statement`](crate::ast::statements::Statement)) |
//! | [`token_stream`](crate::ast::token_stream) | Raw-token-capture helpers (parse "everything up to `;`/`,`", etc.) |
//! | [`tokens`](crate::ast::tokens) | The [`Token!`](crate::Token) macro and the keyword/punctuation token types it expands to |
//! | [`type`](crate::ast::type) | Types ([`type::Type`](crate::ast::type::Type) and ~12 concrete kinds) |
//! | [`visibility`](crate::ast::visibility) | `pub` / `pub(crate)` / `pub(in path)` ([`visibility::Visibility`](crate::ast::visibility::Visibility)) |
//!
//! [`item`](crate::ast::item) additionally declares one submodule per item
//! kind: [`item::struct`](crate::ast::item::struct),
//! [`item::enum_item`](crate::ast::item::enum_item),
//! [`item::function`](crate::ast::item::function),
//! [`item::trait_item`](crate::ast::item::trait_item),
//! [`item::implementation`](crate::ast::item::implementation),
//! [`item::impl_item`](crate::ast::item::impl_item),
//! [`item::use`](crate::ast::item::use),
//! [`item::module`](crate::ast::item::module),
//! [`item::static_item`](crate::ast::item::static_item),
//! [`item::constant`](crate::ast::item::constant),
//! [`item::extern_crate`](crate::ast::item::extern_crate),
//! [`item::extern_block`](crate::ast::item::extern_block),
//! [`item::macro_item`](crate::ast::item::macro_item) and
//! [`item::associated`](crate::ast::item::associated).

pub mod attributes;
pub mod crate_source;
pub mod delimiter;
pub mod expression;
pub mod generics;
/// Identifier classification helpers (crate-private; no public API).
pub mod identifiers;
pub mod item;
pub mod literal;
pub mod path;
pub mod pattern;
pub mod signature;
pub mod statements;
pub mod token_stream;
pub mod tokens;
pub mod r#type;
pub mod visibility;

#[cfg(all(test, feature = "proc-macro2"))]
mod tests;
