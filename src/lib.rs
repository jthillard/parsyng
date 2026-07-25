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
//!   implementing both traits on your own structs field-by-field.
//!
//! # Quick start
//!
//! A minimal function-like macro that doubles an integer literal:
//!
//! ```ignore
//! // in a crate with `[lib] proc-macro = true`
//! #[parsyng::proc_macro]
//! pub fn double(n: u32) -> u32 {
//!     n * 2
//! }
//! ```
//!
//! ```ignore
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
//! ```no_run
//! # // `no_run`: constructing real `proc_macro` tokens outside of an actual
//! # // macro invocation panics unless the `proc-macro2` feature is enabled;
//! # // see "Feature flags" below.
//! use parsyng::quote;
//!
//! let name = "world";
//! let tokens = quote! {
//!     println!("Hello, {}!", #name);
//! };
//! ```
//!
//! `#ident` interpolates a value that implements [`ToTokens`], `#{ expr }`
//! interpolates the result of an arbitrary expression, and `#(...)* ` /
//! `#(...),*` repeats its body once per item yielded by an [`Iterator`]. See
//! [`quote!`] for the full syntax.
//!
//! # Parsing token streams
//!
//! ```no_run
//! # // see the note above `quote!`'s example about `no_run` and `proc-macro2`
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
//! # `syn` vs `parsyng`
//!
//! `parsyng` aims to be as complete as `syn`, but with some useful helpers to reduce the complexity of procedural macros.
//! Currently, here are the differences between `parsyng` and `syn` : 
//!
//! - A single crate with no required external dependency on `syn`/`quote`,
//!   built directly on `proc_macro` (`proc_macro2` is opt-in).
//! - The [`macro@proc_macro`] / [`macro@proc_macro_attribute`] /
//!   [`macro@proc_macro_derive`] helper attributes, which remove almost all of
//!   the boilerplate `syn`/`quote`-based macros still need to hand-write
//!   (parsing the input, matching on the `Result`, converting the output).
//! - More types implements [`Parse`] and `parsyng` provides a [`macro@Parse`]
//!   derive macros to avoid implementing it manually.
//!
//! # Feature flags
//!
//! - **`proc-macro2`** — use the `proc_macro2` crate instead of the
//!   compiler's built-in `proc_macro` for every token type in [`ast`] and
//!   [`quote!`]'s output. Required to call [`quote!`], [`parse_quote!`] or any
//!   [`Parse`]/[`ToTokens`] implementation outside of an actual macro
//!   invocation (for example, in unit tests or a `build.rs`), since the real
//!   `proc_macro` crate panics when used outside the compiler's macro
//!   expansion context.
//! - **`debug-pretty`** — when a macro built with [`macro@proc_macro`] or another helper is
//!   annotated with the `debug` argument (e.g. `#[parsyng::proc_macro(debug)]`),
//!   pipe its generated output through `rustfmt` before printing it, instead of
//!   printing the raw, unformatted token stream. See
//!   `examples/debug-attribute` for a worked example of why this is useful.
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

pub use parsyng_proc_macros::proc_macro_ as proc_macro;
pub use parsyng_proc_macros::proc_macro_attribute_ as proc_macro_attribute;
pub use parsyng_proc_macros::proc_macro_derive_ as proc_macro_derive;
pub use parsyng_proc_macros::{Parse, ToTokens};
