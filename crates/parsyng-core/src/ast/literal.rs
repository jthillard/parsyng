//! Literal parsing.
//!
//! Only numeric literals (integer and float) are implemented; string, char
//! and byte(-string) literals are `todo!()` in [`Literal`]'s [`Parse`] impl.

use core::range::Range;
use std::str::FromStr;

use crate::ToTokens;

use crate::{
    ast::identifiers::is_identifier_or_keyword,
    error::{Diagnostics, Result},
    parse::{Parse, ParseBuffer},
    proc_macro::{self, Span},
};

/// An integer literal, e.g. `0xFFu32`, `1_000`, `0b101`.
///
/// Stores the prefix (`0x`/`0b`/`0o`, if any) and type suffix (if any) as
/// byte ranges into the original literal text; [`content`](Self::content)
/// strips both, leaving just the digits. [`u8`]..[`usize`] each implement
/// [`Parse`] directly on top of this type, additionally validating that the
/// suffix (if present) matches their own type name.
///
/// Reference: <https://doc.rust-lang.org/reference/tokens.html#integer-literals>
#[derive(Debug, Clone)]
pub struct LiteralNumber {
    content: String,
    prefix: Range<usize>,
    suffix: Range<usize>,
    span: Span,
}

/// A floating-point literal, e.g. `1.5`, `1e10`, `1.0f64`.
///
/// Reference: <https://doc.rust-lang.org/reference/tokens.html#floating-point-literals>
#[derive(Debug, Clone)]
pub struct LiteralFloat {
    content: String,
    suffix: Range<usize>,
    span: Span,
}

/// A literal token. Currently only dispatches to [`LiteralNumber`] or
/// [`LiteralFloat`] — see the [module docs](self) for what's missing.
///
/// Reference: <https://doc.rust-lang.org/reference/tokens.html#literals>
#[derive(Debug, Clone)]
pub enum Literal {
    /// An integer literal.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#integer-literals>
    UInt(LiteralNumber),
    /// A floating-point literal.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#floating-point-literals>
    Float(LiteralFloat),
}

impl LiteralNumber {
    /// The digits, excluding any radix prefix or type suffix.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content[self.prefix.end..self.suffix.start]
    }
    /// The radix prefix (`0x`, `0b`, `0o`), or an empty string if there is
    /// none (decimal).
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.content[self.prefix]
    }
    /// The type suffix (e.g. `u32`), or an empty string if there is none.
    #[must_use]
    pub fn suffix(&self) -> &str {
        &self.content[self.suffix]
    }
    /// This literal's span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}
impl LiteralFloat {
    /// The numeric part of the literal, excluding any type suffix.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content[0..self.suffix.start]
    }
    /// The type suffix (e.g. `f64`), or an empty string if there is none.
    #[must_use]
    pub fn suffix(&self) -> &str {
        &self.content[self.suffix]
    }
    /// This literal's span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

macro_rules! unsigned_integer_impls {
    ($($ty:ty,)*) => {
        $(impl Parse for $ty {
            fn parse(input: &mut ParseBuffer) -> Result<Self> {
                input.parse::<LiteralNumber>().and_then(|lit| {
                    if !lit.suffix().is_empty() && lit.suffix() != stringify!($ty) {
                        return Err(Diagnostics::new_error_spanned(format!(concat!("Expected ", stringify!($ty), ", found `{}`"), lit.suffix()), lit.span()));
                    }
                    lit.content().parse::<$ty>().map_err(|err| {
                        Diagnostics::new_error_spanned(format!(concat!("Failed to parse ", stringify!($ty)," literal: {}"), err), lit.span())
                    })
                })
            }
        })*
    };
}

unsigned_integer_impls! {
    u8, u16, u32, u64, u128, usize,
}

impl Parse for Literal {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Some(literal) = input.literal() {
            let literal_str = literal.to_string();

            if literal_str.starts_with('"') {
                todo!()
            } else if literal_str.starts_with('\'') {
                todo!()
            } else if literal_str.starts_with("b\"") {
                todo!()
            } else if literal_str.starts_with("b'") {
                todo!()
            } else if (literal_str.starts_with("0x") || !literal_str.contains(['f', 'e']))
                && !literal_str.contains('.')
            {
                return Ok(Self::UInt(parse_integer_literal(
                    literal_str,
                    literal.span(),
                )?));
            } else if let Ok(float) = parse_float_literal(literal_str, literal.span()) {
                return Ok(Self::Float(float));
            }
        }
        Err(Diagnostics::new_error_spanned(
            "Expected literal",
            input.span(),
        ))
    }
}

impl Parse for LiteralNumber {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Some(literal) = input.literal() {
            let literal_str = literal.to_string();
            return parse_integer_literal(literal_str, literal.span());
        }
        Err(Diagnostics::new_error_spanned(
            "Expected number literal",
            input.span(),
        ))
    }
}

impl Parse for LiteralFloat {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Some(literal) = input.literal() {
            let literal_str = literal.to_string();
            return parse_float_literal(literal_str, literal.span());
        }
        Err(Diagnostics::new_error_spanned(
            "Expected float literal",
            input.span(),
        ))
    }
}

fn byte(bytes: &str, position: usize) -> u8 {
    bytes.as_bytes().get(position).copied().unwrap_or(0)
}

fn parse_integer_literal(literal: String, span: Span) -> Result<LiteralNumber> {
    let s = literal.as_str();
    let len = literal.len();

    let radix;
    let prefix;
    match (byte(s, 0), byte(s, 1)) {
        (b'0', b'b') => {
            radix = 2;
            prefix = 0..2;
        }
        (b'0', b'x') => {
            radix = 16;
            prefix = 0..2;
        }
        (b'0', b'o') => {
            radix = 8;
            prefix = 0..2;
        }
        _ => {
            radix = 10;
            prefix = 0..0;
        }
    }

    let mut position = prefix.len();
    let mut has_digit = false;

    for byte in &s.as_bytes()[position..] {
        match byte {
            c if radix == 16 && c.is_ascii_hexdigit() => {}
            c if radix == 10 && c.is_ascii_digit() => {}
            c if radix == 8 && matches!(c, b'0'..=b'7') => {}
            c if radix == 2 && matches!(c, b'0'..=b'1') => {}
            b'_' => {
                if !has_digit {
                    return Err(Diagnostics::new_error_spanned(
                        "Expected a digit, found `_`",
                        span,
                    ));
                }
            }
            _ => break,
        }
        has_digit = true;
        position += 1;
    }

    let suffix = Range::from(position..len);

    if !suffix.is_empty() && !is_identifier_or_keyword(&s[suffix]) {
        return Err(Diagnostics::new_error_spanned(
            "Expected identifier as integer suffix",
            span,
        ));
    }

    Ok(LiteralNumber {
        content: literal,
        prefix: prefix.into(),
        suffix,
        span,
    })
}

fn parse_float_exponent(s: &str, span: Span) -> Result<usize> {
    let mut position = 0;
    match byte(s, 0) {
        b'e' | b'E' => {
            position += 1;
        }
        _ => {
            return Err(Diagnostics::new_error_spanned(
                "Expected `e` or `E` at the beginning of a float exponent",
                span,
            ));
        }
    }
    match byte(s, 1) {
        b'+' | b'-' => {
            position += 1;
        }
        _ => {}
    }

    let mut has_digit = false;

    for byte in &s.as_bytes()[position..] {
        match byte {
            b'_' => {}
            c if c.is_ascii_digit() => {
                has_digit = true;
            }
            _ => break,
        }
        position += 1;
    }

    if !has_digit {
        return Err(Diagnostics::new_error_spanned(
            "Expected at least one digit after exponent",
            span,
        ));
    }

    Ok(position)
}

fn parse_float_literal(literal: String, span: Span) -> Result<LiteralFloat> {
    let s = literal.as_str();
    let len = literal.len();

    let mut position = 0;

    let mut has_digit = false;
    let mut has_point = false;

    for byte in s.as_bytes() {
        // byte next a `.`
        if has_point && !has_digit && unicode_ident::is_xid_start(*byte as char) {
            return Err(Diagnostics::new_error_spanned(
                format!("Unexpected `{}` after `.` in float literal", *byte as char),
                span,
            ));
        }
        match byte {
            c if c.is_ascii_digit() => {
                position += 1;
                has_digit = true;
            }
            b'.' => {
                if has_point {
                    return Err(Diagnostics::new_error_spanned("Unexpected `.`", span));
                }
                has_point = true;
                position += 1;
                has_digit = false;
            }
            b'e' | b'E' => {
                if !has_digit {
                    return Err(Diagnostics::new_error_spanned(
                        "Expected a digit, found `e`",
                        span,
                    ));
                }
                position += parse_float_exponent(&s[position..], span)?;
                break;
            }
            b'_' => {
                if !has_digit {
                    return Err(Diagnostics::new_error_spanned(
                        "Expected a digit, found `_`",
                        span,
                    ));
                }
                position += 1;
            }
            _ => break,
        }
    }

    let suffix = Range::from(position..len);

    if !suffix.is_empty() && !is_identifier_or_keyword(&s[suffix]) {
        return Err(Diagnostics::new_error_spanned(
            "Expected identifier as float suffix",
            span,
        ));
    }

    Ok(LiteralFloat {
        content: literal,
        suffix,
        span,
    })
}

impl ToTokens for Literal {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::UInt(literal_number) => literal_number.to_tokens(tokens),
            Self::Float(literal_float) => literal_float.to_tokens(tokens),
        }
    }
}

impl ToTokens for LiteralFloat {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        tokens.extend(Some(proc_macro::Literal::from_str(&self.content).unwrap()));
    }
}

impl ToTokens for LiteralNumber {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        tokens.extend(Some(proc_macro::Literal::from_str(&self.content).unwrap()));
    }
}
