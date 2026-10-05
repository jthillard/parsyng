//! `parsyng` is a toolkit for writing Rust procedural macros: a parser for Rust
//! syntax plus a token-stream builder, filling the role that [`syn`] and
//! [`quote`] fill together, but built to be easier to use.
//!
//! [`syn`]: https://docs.rs/syn
//! [`quote`]: https://docs.rs/quote
//!
//! It gives you:
//!
//! - **[`ast`]** — a tree of Rust syntax types (items, expressions, types,
//!   patterns, generics, ...), each implementing [`Parse`] to turn a
//!   [`TokenStream`](proc_macro::TokenStream) into a typed value and [`ToTokens`]
//!   to turn it back into one.
//! - **[`quote!`]** and **[`quote_spanned!`]** — build a token stream from
//!   almost-literal Rust syntax, interpolating `#variable`s and repeating
//!   `#(#items),*` sequences, the same way the `quote` crate does.
//! - **[`macro@proc_macro`]**, **[`macro@proc_macro_attribute`]** and
//!   **[`macro@proc_macro_derive`]** — drop-in replacements for
//!   `#[proc_macro]`, `#[proc_macro_attribute]` and `#[proc_macro_derive]` that
//!   let the annotated function take typed, [`Parse`]-implementing arguments
//!   and return any [`ToTokens`] value, instead of hand-rolling
//!   `proc_macro::TokenStream` parsing and error reporting.
//! - **[`macro@Parse`]** and **[`macro@ToTokens`]** derive macros, for
//!   implementing both traits on your own structs and enums field-by-field.
//!
//! # Quick start
//!
//! A minimal function-like macro that doubles an integer literal:
//!
//! ```
//! // in a crate with `[lib] proc-macro = true`
//! # const IGNORE_TOKENS: &str = stringify! {
//! #[parsyng::proc_macro]
//! # };
//! pub fn double(n: u32) -> u32 {
//!     n * 2
//! }
//! ```
//!
//! ```
//! # fn double(n: u32) -> u32 {
//! #     n * 2
//! # }
//! # macro_rules! double {
//! #     ($n:expr) => {
//! #         double($n)
//! #     };
//! # }
//! // in a crate depending on the macro crate above
//! assert_eq!(double!(21), 42);
//! ```
//!
//! The attribute parses `n` out of the macro's input using [`Parse`] (erroring
//! out with a `compile_error!` if that fails), calls the function body, and
//! turns the returned `u32` back into tokens with [`ToTokens`] — no manual
//! `TokenStream` plumbing required. See [`macro@proc_macro`] for the full
//! picture, including how to accept and return `Result<T, Diagnostics>` for
//! fallible macros.
//!
//! For a `#[derive(...)]`-style macro built directly on the [`ast`] types
//! (the `heapsize` example, ported from `syn`'s own documentation), see
//! `examples/heapsize` in the repository.
//!
//! # Building token streams with `quote!`
//!
//! ```
//! use parsyng::quote;
//!
//! let name = "world";
//! let tokens = quote! {
//!     println!("Hello, {}!", #name);
//! };
//! # assert_eq!(tokens.to_string(), r#"println ! ("Hello, {}!" , "world") ;"#);
//! ```
//!
//! `#ident` interpolates a value that implements [`ToTokens`], `#{ expr }`
//! interpolates the result of an arbitrary expression, and `#(...)* ` /
//! `#(...),*` repeats its body once per item yielded by an [`Iterator`]. See
//! [`quote!`] for the full syntax.
//!
//! # Parsing token streams
//!
//! ```
//! use parsyng::ast::item::ItemStruct;
//! use parsyng::parse::ParseBuffer;
//! use parsyng::quote;
//!
//! let source = quote! {
//!     struct Point { x: f64, y: f64 }
//! };
//!
//! let mut buffer = ParseBuffer::new(source);
//! let item: ItemStruct = buffer.parse().unwrap();
//! assert_eq!(item.ident().to_string(), "Point");
//! ```
//!
//! Every [`ast`] node can be parsed this way. [`Parse`] is also implemented
//! for many standard types ([`u8`]..[`u128`], [`bool`], [`Option<T>`], [`Vec<T>`],
//! tuples, ...) as well as combinators like [`combinator::Punctuated`] and
//! [`combinator::Either`], so custom [`ast`]-like types built out of them get
//! parsing for free.
//!
//! # Why not `syn`/`unsynn`/`moxy`?
//!
//! The most widely used crate for writing procedural macros is `syn`. It is
//! powerful and relatively easy to use, but it also has some flaws this crate
//! tries to fix, without the trade-offs of alternatives like `unsynn` and
//! `moxy`:
//!
//! - **One dependency**: a single crate in your `Cargo.toml`, instead of
//!   `syn`, `quote` and `proc-macro2`.
//! - **Boilerplate**: `syn`/`quote`-based macros need to hand-write a lot of
//!   boilerplate (parsing the input, matching on the `Result`, converting the
//!   output), even for simple macros. The [`macro@proc_macro`] /
//!   [`macro@proc_macro_attribute`] / [`macro@proc_macro_derive`] helper
//!   attributes remove it.
//! - **Speed**: `syn` takes a while to compile, and `moxy` even longer.
//!   `parsyng` compiles significantly faster than `syn`/`quote`, `unsynn` and
//!   `moxy`, and parses faster at runtime too; see
//!   [`BENCH.md`](https://github.com/jthillard/parsyng/blob/main/BENCH.md)
//!   for the numbers.
//! - **Grammar**: unlike `unsynn`, which ships no Rust grammar, `parsyng`
//!   comes with an [`ast`] covering all of stable Rust (with the `full`
//!   feature).
//! - **Simplicity**: `moxy` introduces many new concepts for writing macros.
//!   `parsyng` stays close to the `syn`/`quote` model ([`Parse`],
//!   [`ToTokens`], [`quote!`]), without adding much complexity.
//!
//! # Minimum supported Rust version
//!
//! Rust 1.95 or newer (edition 2024).
//!
//! # Feature flags
//!
//! - **`parsing`** (default) — the [`Parse`] machinery, the [`ast`],
//!   [`parse_quote!`] and the helper attributes. Without it only [`quote!`],
//!   [`quote_spanned!`], [`format_ident!`] and [`ToTokens`] remain.
//! - **`full`** — the whole Rust grammar: expressions, statements, patterns,
//!   function signatures, every item kind (`ast::item::Item`) and whole source
//!   files (`ast::crate_source::Crate`). Without it, the AST covers what derive
//!   macros need (types, paths, generics, `where` clauses, attributes,
//!   visibility, literals, structs and enums, [`DeriveInput`]), which compiles
//!   noticeably faster.
//! - **`extra-traits`** — `Debug` implementations for the AST and combinator
//!   types.
//! - **`fallback`** — use the `parsyng-fallback` crate, a pure-Rust
//!   implementation of the token types (parsyng's counterpart of
//!   `proc-macro2`), instead of the compiler's built-in `proc_macro`. Required
//!   to call [`quote!`], [`parse_quote!`] or any [`Parse`]/[`ToTokens`]
//!   implementation outside of an actual macro invocation (for example, in
//!   unit tests or a `build.rs`), since the real `proc_macro` crate panics
//!   when used outside the compiler's macro expansion context. Unlike
//!   `proc_macro2`, it never forwards to the compiler; a proc-macro crate
//!   built with it still works, but its macros lose spans, so only enable it
//!   for code running outside the compiler (e.g. as a dev-dependency feature
//!   for tests).
//! - **`debug-pretty`** — when a macro built with [`macro@proc_macro`] & co.
//!   is annotated with the `debug` argument (e.g.
//!   `#[parsyng::proc_macro(debug)]`), pipe its generated output through
//!   `rustfmt` before printing it, instead of printing the raw, unformatted
//!   token stream. See
//!   [`examples/debug-attribute`](https://github.com/jthillard/parsyng/tree/main/examples/debug-attribute)
//!   for why this is useful when a macro emits invalid syntax that the Rust
//!   parser itself can't explain.
//!
//! [`DeriveInput`]: ast::item::DeriveInput
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    rustdoc::all,
    rustdoc::redundant_explicit_links,
    invalid_doc_attributes,
    unused_doc_comments,
    missing_docs
)]

pub use parsyng_core::*;
pub use parsyng_quote_macros::{quote, quote_spanned};

/// Build a value of any [`Parse`] type from an almost-literal snippet of
/// Rust syntax, combining [`quote!`] and [`parse::ParseBuffer::parse`] in one
/// step.
///
/// Equivalent to `syn::parse_quote!`: the input accepts the same
/// `#interpolation` syntax as [`quote!`], the resulting tokens are parsed as
/// `T` (inferred from context), and parsing failures (including leftover
/// tokens) panic rather than returning a `Result` — use this for syntax you
/// know must be valid (e.g.
/// building a fixed piece of generated code), not for parsing arbitrary
/// macro input.
///
/// ```
/// use parsyng::ast::r#type::Type;
/// use parsyng::parse_quote;
///
/// let ty: Type = parse_quote!(Vec<u8>);
/// # assert_eq!(parsyng::ToTokens::to_token_stream(&ty).to_string(), "Vec < u8 >");
/// ```
#[cfg(feature = "parsing")]
#[macro_export]
macro_rules! parse_quote {
    ($($t:tt)*) => {{
        $crate::parse::parse_all($crate::quote! { $($t)* }).expect("`parse_quote!` failed to parse its input")
    }};
}

#[cfg(feature = "parsing")]
pub use parsyng_proc_macros::proc_macro_ as proc_macro;
#[cfg(feature = "parsing")]
pub use parsyng_proc_macros::proc_macro_attribute_ as proc_macro_attribute;
#[cfg(feature = "parsing")]
pub use parsyng_proc_macros::proc_macro_derive_ as proc_macro_derive;
#[cfg(feature = "parsing")]
pub use parsyng_proc_macros::{Parse, ToTokens};
