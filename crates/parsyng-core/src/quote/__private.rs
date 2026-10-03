//! Runtime helpers called by the code `quote!`/`quote_spanned!` expand to.
//!
//! Every run of literal tokens (nested groups without interpolations
//! included) is encoded by the macro into a single string literal and
//! rebuilt by [`extend_static`]: the generated code is one call with a
//! constant argument, which keeps it cheap to compile at every
//! optimisation level, and the run is appended with one `extend` per group
//! level (a single compiler round trip instead of one per token).
//!
//! # Encoding
//!
//! A byte string of records, each starting with a tag byte:
//! - `i<len><name>`: identifier, `r<len><name>`: raw identifier (without
//!   `r#`), `l<len><text>`: literal from its source text, where `<len>` is
//!   the byte length of what follows, as two little-endian bytes;
//! - `j<char>`: joint punctuation, `a<char>`: alone punctuation (always
//!   ASCII);
//! - `(`, `[`, `{`, `n`: open a group (`n` for [`Delimiter::None`]);
//! - `)`: close the innermost open group.

pub use crate::proc_macro;
use crate::proc_macro::{
    Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree,
};

/// Decode `ops` (see the [module docs](self)) and append it to `tokens`.
pub fn extend_static(tokens: &mut TokenStream, ops: &'static [u8]) {
    decode(tokens, ops, None);
}

/// [`extend_static`], with every token spanned at `span`.
pub fn extend_static_spanned(tokens: &mut TokenStream, ops: &'static [u8], span: Span) {
    decode(tokens, ops, Some(span));
}

const MALFORMED: &str = "quote!: malformed token table";

/// Split a length-prefixed string off the front of `ops`.
fn text(ops: &'static [u8]) -> (&'static str, &'static [u8]) {
    let [lo, hi, rest @ ..] = ops else {
        panic!("{MALFORMED}")
    };
    let (text, rest) = rest.split_at(usize::from(u16::from_le_bytes([*lo, *hi])));
    (core::str::from_utf8(text).expect(MALFORMED), rest)
}

/// An identifier `quote!` already validated.
#[cfg(not(feature = "fallback"))]
fn ident(name: &str, raw: bool, span: Span) -> Ident {
    if raw {
        Ident::new_raw(name, span)
    } else {
        Ident::new(name, span)
    }
}
/// An identifier `quote!` already validated: no allocation, no validation.
#[cfg(feature = "fallback")]
fn ident(name: &'static str, raw: bool, span: Span) -> Ident {
    Ident::new_static_unchecked(name, raw, span)
}

/// A literal `quote!` already lexed.
#[cfg(not(feature = "fallback"))]
fn literal(text: &str, span: Option<Span>) -> Literal {
    let mut literal = text.parse::<Literal>().expect("quote!: invalid literal");
    if let Some(span) = span {
        literal.set_span(span);
    }
    literal
}
/// A literal `quote!` already lexed: no allocation, no lexing.
#[cfg(feature = "fallback")]
fn literal(text: &'static str, span: Option<Span>) -> Literal {
    Literal::new_static_unchecked(text, span.unwrap_or_else(Span::call_site))
}

#[inline(never)]
fn decode(tokens: &mut TokenStream, mut ops: &'static [u8], span: Option<Span>) {
    // Every tree not yet in a group, innermost group last; `levels` holds
    // each open group's delimiter and the index its trees start at. The
    // whole run is sent with one `extend`: a single round trip to the
    // compiler.
    let mut trees: Vec<TokenTree> = Vec::new();
    let mut levels: Vec<(Delimiter, usize)> = Vec::new();
    while let [tag, rest @ ..] = ops {
        ops = rest;
        let tree: TokenTree = match tag {
            b'i' | b'r' => {
                let (name, rest) = text(ops);
                ops = rest;
                ident(name, *tag == b'r', span.unwrap_or_else(Span::call_site)).into()
            }
            b'j' | b'a' => {
                let [ch, rest @ ..] = ops else {
                    panic!("{MALFORMED}")
                };
                ops = rest;
                let spacing = if *tag == b'j' {
                    Spacing::Joint
                } else {
                    Spacing::Alone
                };
                let mut punct = Punct::new(char::from(*ch), spacing);
                if let Some(span) = span {
                    punct.set_span(span);
                }
                punct.into()
            }
            b'l' => {
                let (text, rest) = text(ops);
                ops = rest;
                literal(text, span).into()
            }
            b')' => {
                let (delimiter, start) = levels.pop().expect(MALFORMED);
                let mut group = Group::new(delimiter, trees.drain(start..).collect());
                if let Some(span) = span {
                    group.set_span(span);
                }
                group.into()
            }
            open => {
                let delimiter = match open {
                    b'(' => Delimiter::Parenthesis,
                    b'[' => Delimiter::Bracket,
                    b'{' => Delimiter::Brace,
                    _ => Delimiter::None,
                };
                levels.push((delimiter, trees.len()));
                continue;
            }
        };
        if ops.is_empty() && trees.is_empty() {
            // A single top-level tree: skip the intermediate vector.
            tokens.extend(core::iter::once(tree));
            return;
        }
        if trees.capacity() == 0 {
            // Every record takes at least two bytes.
            trees.reserve(ops.len() / 2 + 1);
        }
        trees.push(tree);
    }
    tokens.extend(trees);
}

/// Append a single tree to `tokens`.
pub fn push(tokens: &mut TokenStream, tree: TokenTree) {
    tokens.extend(core::iter::once(tree));
}

/// A delimited group spanned at [`Span::call_site`].
#[must_use]
pub fn group(delimiter: Delimiter, stream: TokenStream) -> TokenTree {
    Group::new(delimiter, stream).into()
}

/// A delimited group.
#[must_use]
pub fn group_spanned(delimiter: Delimiter, stream: TokenStream, span: Span) -> TokenTree {
    let mut group = Group::new(delimiter, stream);
    group.set_span(span);
    group.into()
}
