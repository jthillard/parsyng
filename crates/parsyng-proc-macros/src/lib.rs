//! Implementation of the [`proc_macro`](proc_macro_), [`proc_macro_attribute`](proc_macro_attribute_)
//! and the [`proc_macro_derive`](proc_macro_derive_) procedural macros, and [`Parse`], [`ToTokens`] derive macros for `parsyng`.
//!
//! This crate only depends on the compiler's `proc_macro`, not on
//! `parsyng-core`, so that both compile in parallel: it walks just the
//! outline of its input (see `tokens`) and passes every other token through.

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

use proc_macro::{Ident, Literal, Span, TokenStream};

mod derive_parse;
mod derive_to_tokens;
mod helper_common;
mod proc_macro_attribute_helper;
mod proc_macro_derive_helper;
mod proc_macro_helper;
mod tokens;

use tokens::Out;

/// `parsyng::debug_stream("<macro>", "<location>", &output);`
pub(crate) fn dbg_macros(macro_name: &Ident) -> Out {
    let location = format!(
        "{}:{}:{}",
        Span::call_site().file(),
        Span::call_site().line(),
        Span::call_site().column()
    );
    let mut args = Out::new();
    args.tree(Literal::string(&macro_name.to_string()))
        .src(",")
        .tree(Literal::string(&location))
        .src(", &output");
    let mut out = Out::new();
    out.src("parsyng::debug_stream")
        .group(proc_macro::Delimiter::Parenthesis, args)
        .src(";");
    out
}

/// Helper attribute to build new procedural macros. This replaces the
/// standard library's `#[proc_macro]` attribute.
///
/// It differs from the standard library's by allowing any input type that
/// implements the [`Parse`](https://docs.rs/parsyng/latest/parsyng/parse/trait.Parse.html) trait — the input is parsed
/// automatically, and a parse failure is turned into a `compile_error!` at
/// the offending span instead of panicking the proc-macro process. It allows
/// any output type that implements [`ToTokens`](https://docs.rs/parsyng/latest/parsyng/trait.ToTokens.html),
/// automatically converting it into a [`TokenStream`]. Since
/// [`Result<T, E>`](Result) implements [`ToTokens`](https://docs.rs/parsyng/latest/parsyng/trait.ToTokens.html)
/// whenever `T` and `E` do (and `error::Diagnostics` implements it too), the
/// annotated function can return `error::Result<T>` to fail with a spanned
/// diagnostic from inside the macro body too, not just during argument
/// parsing.
///
/// # Example
/// ```
/// # const IGNORE_TOKENS: &str = stringify! {
/// #[parsyng::proc_macro]
/// # };
/// pub fn add_one(n: u8) -> u8 {
///    n + 1
/// }
/// ```
/// and then
/// ```
/// # fn add_one(n: u8) -> u8 {
/// #    n + 1
/// # }
/// # macro_rules! add_one {
/// #     ($n:expr) => {
/// #         add_one($n)
/// #     };
/// # }
/// println!("{}", add_one!(5));
/// // Output : 6
/// # assert_eq!(add_one!(5), 6);
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
    match proc_macro_helper::proc_macro(args, input) {
        Ok(ok) => ok,
        Err(err) => err.into_compile_error(),
    }
}

/// Helper attribute to build new procedural macro attributes. This replaces
/// the standard library's `#[proc_macro_attribute]` attribute.
///
/// Like [`proc_macro`](proc_macro_), it lets the annotated function take
/// typed, [`Parse`](https://docs.rs/parsyng/latest/parsyng/parse/trait.Parse.html)-implementing arguments — one for
/// the attribute's own arguments (`attr` in `#[my_attr(attr)] item`), one
/// for the annotated item — and return any
/// [`ToTokens`](https://docs.rs/parsyng/latest/parsyng/trait.ToTokens.html) value, instead of manually parsing
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
/// # const IGNORE_TOKENS: &str = stringify! {
/// #[parsyng::proc_macro_attribute]
/// # };
/// pub fn my_attribute(_attr: (), item: Item) -> Result<Item> {
///     // inspect or rewrite `item` here
///     Ok(item)
/// }
/// # let item = my_attribute((), parsyng::parse_quote!(struct S;)).unwrap();
/// # assert_eq!(parsyng::ToTokens::to_token_stream(&item).to_string(), "struct S ;");
/// ```
// Export with an underscore, since it will conflicts with the `proc_macro_attribute` builtin.
#[proc_macro_attribute]
pub fn proc_macro_attribute_(args: TokenStream, input: TokenStream) -> TokenStream {
    match proc_macro_attribute_helper::proc_macro_attribute(args, input) {
        Ok(ok) => ok,
        Err(err) => err.into_compile_error(),
    }
}

/// Helper attribute to build new derive macros. This replaces the standard
/// library's `#[proc_macro_derive]` attribute.
///
/// Like [`proc_macro`](proc_macro_), it lets the annotated function take a
/// single typed, [`Parse`](https://docs.rs/parsyng/latest/parsyng/parse/trait.Parse.html)-implementing argument —
/// typically [`ast::item::DeriveInput`](https://docs.rs/parsyng/latest/parsyng/ast/item/type.DeriveInput.html)
/// — and return any [`ToTokens`](https://docs.rs/parsyng/latest/parsyng/trait.ToTokens.html) value.
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
/// # const IGNORE_TOKENS: &str = stringify! {
/// #[parsyng::proc_macro_derive(MyTrait)]
/// # };
/// pub fn derive_my_trait(input: DeriveInput) -> TokenStream {
///     let name = input.ident();
///     parsyng::quote! {
///         impl MyTrait for #name {}
///     }
/// }
/// # let output = derive_my_trait(parsyng::parse_quote!(struct Foo;));
/// # assert_eq!(output.to_string(), "impl MyTrait for Foo { }");
/// ```
// Export with an underscore, since it will conflicts with the `proc_macro_derive` builtin.
#[proc_macro_attribute]
pub fn proc_macro_derive_(args: TokenStream, input: TokenStream) -> TokenStream {
    match proc_macro_derive_helper::proc_macro_derive(args, input) {
        Ok(ok) => ok,
        Err(err) => err.into_compile_error(),
    }
}

/// Derives [`Parse`](https://docs.rs/parsyng/latest/parsyng/parse/trait.Parse.html) by parsing each field, in
/// declaration order, with its own [`Parse`](https://docs.rs/parsyng/latest/parsyng/parse/trait.Parse.html)
/// implementation.
///
/// Equivalent to writing, for `struct Foo { a: A, b: B }`:
///
/// ```
/// # use parsyng::proc_macro::Ident;
/// # struct Foo { a: u8, b: Ident }
/// impl parsyng::parse::Parse for Foo {
///     fn parse(input: &mut parsyng::parse::ParseBuffer) -> parsyng::error::Result<Self> {
///         Ok(Self {
///             a: input.parse()?,
///             b: input.parse()?,
///         })
///     }
/// }
/// # let foo: Foo = parsyng::parse_quote!(1 x);
/// # assert_eq!((foo.a, foo.b.to_string().as_str()), (1, "x"));
/// ```
///
/// Tuple structs parse their fields positionally, and unit structs parse
/// nothing. On an enum, each variant is tried in declaration order (without
/// consuming any input on failure) and the first one that parses wins, so
/// put more specific variants first; a unit variant always matches.
/// Generic parameters and `where` clauses are carried over to the impl.
///
/// See [`macro@ToTokens`] for the complementary derive.
#[proc_macro_derive(Parse)]
pub fn derive_parse(input: TokenStream) -> TokenStream {
    match derive_parse::derive_parse(input) {
        Ok(ok) => ok,
        Err(err) => err.into_compile_error(),
    }
}

/// Derives [`ToTokens`](https://docs.rs/parsyng/latest/parsyng/trait.ToTokens.html) by appending each field's
/// own tokens, in declaration order.
///
/// Equivalent to writing, for `struct Foo { a: A, b: B }`:
///
/// ```
/// # use parsyng::proc_macro::Ident;
/// # struct Foo { a: u8, b: Ident }
/// impl parsyng::ToTokens for Foo {
///     fn to_tokens(&self, tokens: &mut parsyng::proc_macro::TokenStream) {
///         parsyng::ToTokens::to_tokens(&self.a, tokens);
///         parsyng::ToTokens::to_tokens(&self.b, tokens);
///     }
/// }
/// # let foo = Foo { a: 1, b: parsyng::format_ident!("x") };
/// # assert_eq!(parsyng::ToTokens::to_token_stream(&foo).to_string(), "1u8 x");
/// ```
///
/// Tuple structs emit their fields positionally, unit structs emit nothing,
/// and an enum emits the fields of whichever variant it holds (the variant
/// name itself is not emitted). Generic parameters and `where` clauses are
/// carried over to the impl.
///
/// See [`macro@Parse`] for the complementary derive.
#[proc_macro_derive(ToTokens)]
pub fn derive_to_tokens(input: TokenStream) -> TokenStream {
    match derive_to_tokens::derive_to_tokens(input) {
        Ok(ok) => ok,
        Err(err) => err.into_compile_error(),
    }
}
