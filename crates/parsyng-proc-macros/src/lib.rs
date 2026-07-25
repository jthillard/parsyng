//! Implementation of the [`proc_macro`](proc_macro_), [`proc_macro_attribute`](proc_macro_attribute_)
//! and the [`proc_macro_derive`](proc_macro_derive_) procedural macros, and [`Parse`], [`ToTokens`] derive macros for `parsyng`.

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
// We need to add `.into()` due to the `proc-macro2` feature.
#![allow(clippy::useless_conversion)]

use parsyng_core as parsyng;

use parsyng_core::quote;
use proc_macro::{Span, TokenStream};

mod derive_parse;
mod derive_to_tokens;
mod proc_macro_attribute_helper;
mod proc_macro_derive_helper;
mod proc_macro_helper;

/// Create the debug call used to print the macro output if the user added the `debug` attribute.
pub(crate) fn dbg_macros(
    macro_name: &parsyng_core::proc_macro::Ident,
) -> parsyng_core::proc_macro::TokenStream {
    let location = &format!(
        "{}:{}:{}",
        Span::call_site().file(),
        Span::call_site().line(),
        Span::call_site().column()
    );
    quote! {
        parsyng::debug_stream(#{ macro_name.to_string() }, #location, &output);
    }
}

/// Helper attribute to build new procedural macros. This replaces the
/// standard library's `#[proc_macro]` attribute.
///
/// It differs from the standard library's by allowing any input type that
/// implements the [`Parse`](parsyng_core::Parse) trait — the input is parsed
/// automatically, and a parse failure is turned into a `compile_error!` at
/// the offending span instead of panicking the proc-macro process. It allows
/// any output type that implements [`ToTokens`](parsyng_core::ToTokens),
/// automatically converting it into a [`TokenStream`]. Since
/// [`Result<T, E>`](Result) implements [`ToTokens`](parsyng_core::ToTokens)
/// whenever `T` and `E` do (and `error::Diagnostics` implements it too), the
/// annotated function can return `error::Result<T>` to fail with a spanned
/// diagnostic from inside the macro body too, not just during argument
/// parsing.
///
/// # Example
/// ```
/// #[parsyng::proc_macro]
/// pub fn add_one(n: u8) -> u8 {
///    n + 1
/// }
/// ```
/// and then
/// ```
/// println!("{}", add_one!(5));
/// // Output : 6
/// ```
///
/// # The `debug` argument
///
/// `#[parsyng::proc_macro(debug)]` prints the macro's generated output to
/// stderr at every call site, which is invaluable when the macro emits
/// syntax invalid enough that `cargo expand` itself can't recover — see
/// `examples/debug-attribute` for a worked example. Enable the
/// `debug-pretty` feature on `parsyng` to have that output passed through
/// `rustfmt` first.
// Export with an underscore, since it will conflicts with the `proc_macro` builtin.
#[proc_macro_attribute]
pub fn proc_macro_(args: TokenStream, input: TokenStream) -> TokenStream {
    match proc_macro_helper::proc_macro(args.into(), input.into()) {
        Ok(ok) => ok,
        Err(err) => parsyng_core::ToTokens::to_token_stream(&err),
    }
    .into()
}

/// Helper attribute to build new procedural macro attributes. This replaces
/// the standard library's `#[proc_macro_attribute]` attribute.
///
/// Like [`proc_macro`](proc_macro_), it lets the annotated function take
/// typed, [`Parse`](parsyng_core::Parse)-implementing arguments — one for
/// the attribute's own arguments (`attr` in `#[my_attr(attr)] item`), one
/// for the annotated item — and return any
/// [`ToTokens`](parsyng_core::ToTokens) value, instead of manually parsing
/// two `proc_macro::TokenStream`s and matching on the results.
///
/// Accepts the same optional `debug` argument as
/// [`proc_macro`](proc_macro_) (see its documentation for details).
///
/// # Example
/// ```
/// use parsyng::ast::item::Item;
/// use parsyng::error::Result;
///
/// #[parsyng::proc_macro_attribute]
/// pub fn my_attribute(_attr: (), item: Item) -> Result<Item> {
///     // inspect or rewrite `item` here
///     Ok(item)
/// }
/// ```
// Export with an underscore, since it will conflicts with the `proc_macro_attribute` builtin.
#[proc_macro_attribute]
pub fn proc_macro_attribute_(args: TokenStream, input: TokenStream) -> TokenStream {
    match proc_macro_attribute_helper::proc_macro_attribute(args.into(), input.into()) {
        Ok(ok) => ok,
        Err(err) => parsyng_core::ToTokens::to_token_stream(&err),
    }
    .into()
}

/// Helper attribute to build new derive macros. This replaces the standard
/// library's `#[proc_macro_derive]` attribute.
///
/// Like [`proc_macro`](proc_macro_), it lets the annotated function take a
/// single typed, [`Parse`](parsyng_core::Parse)-implementing argument —
/// typically [`ast::item::DeriveInput`](parsyng_core::ast::item::DeriveInput)
/// — and return any [`ToTokens`](parsyng_core::ToTokens) value.
///
/// The attribute's argument names the derive trait, exactly as with the
/// standard library's version: `#[parsyng::proc_macro_derive(MyTrait)]`.
/// The optional `debug` argument is passed after a comma, as with
/// `#[proc_macro_derive(MyTrait, attributes(...))]` in the standard library —
/// here `#[parsyng::proc_macro_derive(MyTrait, debug)]` (see
/// [`proc_macro`](proc_macro_) for what `debug` does).
///
/// # Example
/// ```
/// use parsyng::ast::item::DeriveInput;
/// use parsyng::proc_macro::TokenStream;
///
/// #[parsyng::proc_macro_derive(MyTrait)]
/// pub fn derive_my_trait(input: DeriveInput) -> TokenStream {
///     let name = input.ident();
///     parsyng::quote! {
///         impl MyTrait for #name {}
///     }
/// }
/// ```
// Export with an underscore, since it will conflicts with the `proc_macro_derive` builtin.
#[proc_macro_attribute]
pub fn proc_macro_derive_(args: TokenStream, input: TokenStream) -> TokenStream {
    match proc_macro_derive_helper::proc_macro_derive(args.into(), input.into()) {
        Ok(ok) => ok,
        Err(err) => parsyng_core::ToTokens::to_token_stream(&err),
    }
    .into()
}

/// Derives [`Parse`](parsyng_core::Parse) for a struct with named fields by
/// parsing each field, in declaration order, with its own
/// [`Parse`](parsyng_core::Parse) implementation.
///
/// Equivalent to writing, for `struct Foo { a: A, b: B }`:
///
/// ```ignore
/// impl Parse for Foo {
///     fn parse(input: &mut parsyng::parse::ParseBuffer) -> parsyng::error::Result<Self> {
///         Ok(Self {
///             a: input.parse()?,
///             b: input.parse()?,
///         })
///     }
/// }
/// ```
///
/// Tuple structs and unit structs are not yet supported (`todo!()` in the
/// implementation).
///
/// See [`macro@ToTokens`] for the complementary derive.
#[proc_macro_derive(Parse)]
pub fn derive_parse(input: TokenStream) -> TokenStream {
    match derive_parse::derive_parse(input.into()) {
        Ok(ok) => ok,
        Err(err) => parsyng_core::ToTokens::to_token_stream(&err),
    }
    .into()
}

/// Derives [`ToTokens`](parsyng_core::ToTokens) for a struct with named
/// fields by appending each field's own tokens, in declaration order.
///
/// Equivalent to writing, for `struct Foo { a: A, b: B }`:
///
/// ```ignore
/// impl ToTokens for Foo {
///     fn to_tokens(&self, tokens: &mut parsyng::proc_macro::TokenStream) {
///         self.a.to_tokens(tokens);
///         self.b.to_tokens(tokens);
///     }
/// }
/// ```
///
/// Tuple structs and unit structs are not yet supported (`todo!()` in the
/// implementation).
///
/// See [`macro@Parse`] for the complementary derive.
#[proc_macro_derive(ToTokens)]
pub fn derive_to_tokens(input: TokenStream) -> TokenStream {
    match derive_to_tokens::derive_to_tokens(input.into()) {
        Ok(ok) => ok,
        Err(err) => parsyng_core::ToTokens::to_token_stream(&err),
    }
    .into()
}
