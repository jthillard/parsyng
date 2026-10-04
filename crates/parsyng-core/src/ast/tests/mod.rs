//! Round-trip tests for the AST: parse a token stream as `T`, check that the
//! whole input was consumed, print it back with `ToTokens` and compare with
//! the input. Per-module tests check the parsed structure on top of that.

use crate as parsyng;

use crate::ToTokens;
use crate::{
    ast::delimiter::{Braced, Bracketed, Parenthesized},
    ast::r#type::Type,
    parse::{Parse, ParseBuffer, Peek, parse_all},
    proc_macro::TokenStream,
};
use parsyng_quote_macros::quote;

mod attributes_vis;
mod derive;
#[cfg(feature = "full")]
mod expressions;
#[cfg(feature = "full")]
mod items;
mod literals;
mod paths_generics;
mod rejection;
#[cfg(feature = "full")]
mod signatures;
#[cfg(feature = "full")]
mod statements;
mod types;

/// Lex `input`, for tokens `quote!` can't write (`.0`, raw strings, ...).
pub fn ts(input: &str) -> TokenStream {
    input.parse().unwrap()
}

/// Parse all of `tokens` as `T`.
pub fn parse_exact<T: Parse>(tokens: TokenStream) -> T {
    let mut input = ParseBuffer::new(tokens);
    let source = rest(&input);
    let value = match input.parse::<T>() {
        Ok(value) => value,
        Err(err) => panic!("`{source}` failed to parse: {}", err.to_token_stream()),
    };
    assert!(
        input.is_empty(),
        "`{source}`: tokens left: `{}`",
        rest(&input)
    );
    value
}

/// Parse all of `tokens` as `T` and check it prints back to the same tokens.
pub fn check<T: Parse + ToTokens>(tokens: TokenStream) -> T {
    let expected = tokens.to_string();
    let parsed = parse_exact::<T>(tokens);
    let mut out = TokenStream::new();
    parsed.to_tokens(&mut out);
    assert_eq!(out.to_string(), expected);
    parsed
}

/// [`check`] on a lexed string.
pub fn check_str<T: Parse + ToTokens>(input: &str) -> T {
    check(ts(input))
}

/// The tokens left in `input`, printed.
pub fn rest(input: &ParseBuffer) -> String {
    input.clone().collect::<TokenStream>().to_string()
}

/// Parse a `T` from the front of `tokens`, returning it with the tokens it
/// left behind, printed without whitespace (spacing differs between lexers).
pub fn parse_prefix<T: Parse>(tokens: TokenStream) -> (T, String) {
    let mut input = ParseBuffer::new(tokens);
    let value = input.parse::<T>().unwrap();
    (value, rest(&input).split_whitespace().collect())
}

/// Assert that `tokens` is not a `T`: either `T` fails, or it leaves tokens
/// behind.
pub fn fails<T: Parse>(tokens: TokenStream) {
    let printed = tokens.to_string();
    assert!(
        parse_all::<T>(tokens).is_err(),
        "`{printed}` should not parse as `{}`",
        core::any::type_name::<T>()
    );
}

/// Assert that a [`Peek`] type fails on `tokens` without consuming any of
/// them (as its contract requires), even outside of `try_parse`.
pub fn peek_fails<T: Peek>(tokens: TokenStream) {
    let mut input = ParseBuffer::new(tokens);
    let before = rest(&input);
    assert!(
        input.parse::<T>().is_err(),
        "`{before}` should not parse as `{}`",
        core::any::type_name::<T>()
    );
    assert_eq!(rest(&input), before);
}

#[test]
fn token_stream_nodes() {
    check::<TokenStream>(quote! { a + b });
    check::<crate::proc_macro::TokenTree>(quote! { a });
    check::<crate::proc_macro::Group>(quote! { (a) });
    check::<crate::proc_macro::Ident>(quote! { ident });
    check::<crate::proc_macro::Punct>(quote! { + });
}

#[test]
fn delimiter_nodes() {
    check::<Bracketed<Type>>(quote! { [foo] });
    check::<Braced<Vec<crate::proc_macro::Ident>>>(quote! { { a b } });
    check::<Parenthesized<Type>>(quote! { (foo) });
}
