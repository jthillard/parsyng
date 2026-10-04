//! A tree of Rust syntax types, each implementing
//! [`Parse`](crate::parse::Parse) to build itself from a
//! [`ParseBuffer`](crate::parse::ParseBuffer) and
//! [`ToTokens`](crate::ToTokens) to turn itself back into tokens.
//!
//! There is no umbrella `Item`/`Expr`/`Type` prelude: every type lives at its
//! full path (`ast::item::r#struct::Struct`, `ast::expression::IfExpression`,
//! ...). The most commonly needed entry points are re-exported as `pub type`
//! aliases from [`item`] —
//! [`item::ItemStruct`],
//! [`item::ItemEnum`],
//! [`item::ItemFunction`], etc. — and
//! [`item::DeriveInput`], used as the input type of `#[derive(...)]` macros.
//!
//! # Coverage
//!
//! Coverage is the full stable Rust grammar: items (function, trait, `impl`,
//! inline `mod` and `extern` block bodies included), generics and `where`
//! clauses, types, every literal kind, expressions (including closures,
//! `async`/`const`/`unsafe` blocks, let-chains, qualified paths, raw
//! borrows and expression attributes), statements and patterns. The
//! nightly `const` syntax is covered too: `~const`/`[const]`/`const` trait
//! bounds, `impl const Trait`, `const trait`, const closures, inline
//! `const { ... }` patterns and generic const items, as well as closure
//! binders (`for<'a> |x| ...`), `default` impl items, extern types, named
//! C-variadics and negative bounds.
//!
//! The only token runs kept unparsed are attribute contents, macro bodies,
//! and the two derive-level expressions that must stay available without
//! `full`: array type lengths (`[T; N]`) and enum discriminants. The known
//! gaps are:
//!
//! - Keywords are accepted wherever an identifier is expected (e.g.
//!   `struct fn;` parses), except as the first segment of a macro
//!   invocation's path.
//! - Other unstable syntax (`try`/`yeet` blocks, `box` expressions and
//!   patterns, `macro` items with a separate argument list, ...) is not
//!   parsed.
//!
//! Unsupported syntax is reported as a regular [`Parse`](crate::parse::Parse)
//! error spanned at the offending token; it never panics.
//!
//! Expressions, statements, patterns, function signatures, the [`item::Item`]
//! enum (and every item kind but structs, enums, unions and macro
//! invocations) and whole source files need the `full` feature. Without it,
//! the AST covers what derive macros need: types, paths, generics, `where`
//! clauses, attributes, visibility, literals, structs, enums, unions and
//! [`item::DeriveInput`]. `Debug` implementations need the `extra-traits`
//! feature.
//!
//! # Module map
//!
//! | Module | Contents |
//! | --- | --- |
//! | [`attributes`] | `#[...]` / `#![...]` attributes |
//! | [`crate_source`] | A whole source file ([`crate_source::Crate`]) |
//! | [`delimiter`] | Generic `[T]`/`{T}`/`(T)` wrappers ([`delimiter::Bracketed`], [`delimiter::Braced`], [`delimiter::Parenthesized`]) |
//! | [`expression`] | Expressions ([`expression::Expression`] and ~20 concrete kinds) |
//! | [`generics`] | `impl`/type-position generics views for codegen ([`generics::ImplGenerics`], [`generics::TypeGenerics`]) |
//! | [`item`] | Top-level items ([`item::Item`]), generics, where-clauses, and the per-kind submodules below |
//! | [`literal`] | Numeric literals |
//! | [`path`] | Paths and generic arguments ([`path::SimplePath`], [`path::GenericArgs`]) |
//! | [`pattern`] | Patterns ([`pattern::Pattern`]) |
//! | [`signature`] | Function signatures ([`signature::FnSignature`]) |
//! | [`statements`] | Block statements ([`statements::Statement`]) |
//! | [`token_stream`] | Raw-token-capture helpers (parse "everything up to `;`/`,`", etc.) |
//! | [`tokens`] | The [`Token!`](crate::Token) macro and the keyword/punctuation token types it expands to |
//! | [`type`] | Types ([`type::Type`] and ~12 concrete kinds) |
//! | [`visibility`] | `pub` / `pub(crate)` / `pub(in path)` ([`visibility::Visibility`]) |
//!
//! [`item`] additionally declares one submodule per item
//! kind: [`item::struct`],
//! [`item::enum_item`],
//! [`item::union_item`],
//! [`item::function`],
//! [`item::trait_item`],
//! [`item::implementation`],
//! [`item::impl_item`],
//! [`item::use`],
//! [`item::module`],
//! [`item::static_item`],
//! [`item::constant`],
//! [`item::extern_crate`],
//! [`item::extern_block`],
//! [`item::macro_item`] and
//! [`item::associated`].

pub mod attributes;
#[cfg(feature = "full")]
pub mod crate_source;
pub mod delimiter;
#[cfg(feature = "full")]
pub mod expression;
pub mod generics;
/// Identifier classification helpers (crate-private; no public API).
pub mod identifiers;
pub mod item;
pub mod literal;
pub mod path;
#[cfg(feature = "full")]
pub mod pattern;
#[cfg(feature = "full")]
pub mod signature;
#[cfg(feature = "full")]
pub mod statements;
pub mod token_stream;
pub mod tokens;
pub mod r#type;
pub mod visibility;

#[cfg(all(test, feature = "fallback"))]
mod tests;
