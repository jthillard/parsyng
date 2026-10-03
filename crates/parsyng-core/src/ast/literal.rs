//! Literal parsing.
//!
//! Every kind of Rust literal token is covered: integers ([`LiteralNumber`]),
//! floats ([`LiteralFloat`]), strings ([`LiteralStr`]), byte strings
//! ([`LiteralByteStr`]), C strings ([`LiteralCStr`]), characters
//! ([`LiteralChar`]) and bytes ([`LiteralByte`]), including raw strings
//! (`r#"..."#`) and literal suffixes. [`Literal`] dispatches to the right one.
//!
//! The string-like literals keep their original token, so they re-emit
//! exactly as written (same escapes, same span), and expose their unescaped
//! contents through `value()`. `true`/`false` are identifiers rather than
//! literal tokens, so they are not covered here — but [`bool`] implements
//! [`Parse`] directly, like [`String`], [`char`] and the unsigned integers.

use core::ops::Range;
use std::ffi::{CStr, CString};
use std::str::FromStr;

use crate::ToTokens;

use crate::{
    ast::identifiers::is_identifier_or_keyword,
    error::{Diagnostics, Result},
    parse::{Parse, ParseBuffer, Peek},
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

/// Any literal token.
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
    /// A string literal, e.g. `"foo"` or `r#"foo"#`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#string-literals>
    Str(LiteralStr),
    /// A byte string literal, e.g. `b"foo"` or `br"foo"`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#byte-string-literals>
    ByteStr(LiteralByteStr),
    /// A C string literal, e.g. `c"foo"` or `cr"foo"`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#c-string-literals>
    CStr(LiteralCStr),
    /// A character literal, e.g. `'a'`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#character-literals>
    Char(LiteralChar),
    /// A byte literal, e.g. `b'a'`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#byte-literals>
    Byte(LiteralByte),
}

impl Literal {
    /// This literal's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::UInt(literal) => literal.span(),
            Self::Float(literal) => literal.span(),
            Self::Str(literal) => literal.span(),
            Self::ByteStr(literal) => literal.span(),
            Self::CStr(literal) => literal.span(),
            Self::Char(literal) => literal.span(),
            Self::Byte(literal) => literal.span(),
        }
    }
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
        &self.content[self.prefix.clone()]
    }
    /// The type suffix (e.g. `u32`), or an empty string if there is none.
    #[must_use]
    pub fn suffix(&self) -> &str {
        &self.content[self.suffix.clone()]
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
        &self.content[self.suffix.clone()]
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
        let Some(literal) = input.peek_literal() else {
            return Err(Diagnostics::new_error_spanned(
                "Expected literal",
                input.span(),
            ));
        };
        match QuotedKind::of(&literal.to_string()) {
            Some(QuotedKind::Str) => input.parse().map(Self::Str),
            Some(QuotedKind::ByteStr) => input.parse().map(Self::ByteStr),
            Some(QuotedKind::CStr) => input.parse().map(Self::CStr),
            Some(QuotedKind::Char) => input.parse().map(Self::Char),
            Some(QuotedKind::Byte) => input.parse().map(Self::Byte),
            None => {
                let Some(literal) = input.literal() else {
                    unreachable!("a literal was just peeked")
                };
                let literal_str = literal.to_string();
                if is_float_literal(&literal_str) {
                    parse_float_literal(literal_str, literal.span()).map(Self::Float)
                } else {
                    parse_integer_literal(literal_str, literal.span()).map(Self::UInt)
                }
            }
        }
    }
}

/// Whether a numeric literal's text denotes a float rather than an integer:
/// a decimal literal whose leading digits are followed by `.`, an exponent,
/// or an `f*` type suffix.
fn is_float_literal(literal: &str) -> bool {
    if literal.starts_with("0x") || literal.starts_with("0o") || literal.starts_with("0b") {
        return false;
    }
    let rest = literal.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
    rest.starts_with(['.', 'e', 'E']) || matches!(rest, "f16" | "f32" | "f64" | "f128")
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

    let (radix, prefix) = match (byte(s, 0), byte(s, 1)) {
        (b'0', b'b') => (2, 0..2),
        (b'0', b'x') => (16, 0..2),
        (b'0', b'o') => (8, 0..2),
        _ => (10, 0..0),
    };

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

    let suffix = position..len;

    if !suffix.is_empty() && !is_identifier_or_keyword(&s[suffix.clone()]) {
        return Err(Diagnostics::new_error_spanned(
            "Expected identifier as integer suffix",
            span,
        ));
    }

    Ok(LiteralNumber {
        content: literal,
        prefix,
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

    let suffix = position..len;

    if !suffix.is_empty() && !is_identifier_or_keyword(&s[suffix.clone()]) {
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
            Self::Str(literal) => literal.to_tokens(tokens),
            Self::ByteStr(literal) => literal.to_tokens(tokens),
            Self::CStr(literal) => literal.to_tokens(tokens),
            Self::Char(literal) => literal.to_tokens(tokens),
            Self::Byte(literal) => literal.to_tokens(tokens),
        }
    }
}

/// Re-create a numeric literal token from its (already validated) text.
fn numeric_token(content: &str, span: Span) -> proc_macro::Literal {
    let mut literal = proc_macro::Literal::from_str(content)
        .unwrap_or_else(|_| unreachable!("numeric literal text was validated while parsing"));
    literal.set_span(span);
    literal
}

impl ToTokens for LiteralFloat {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        tokens.extend(Some(numeric_token(&self.content, self.span)));
    }
}

impl ToTokens for LiteralNumber {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        tokens.extend(Some(numeric_token(&self.content, self.span)));
    }
}

/// The kind of a quoted (string-like or character-like) literal token,
/// determined from its prefix.
#[derive(Clone, Copy, PartialEq, Eq)]
enum QuotedKind {
    Str,
    ByteStr,
    CStr,
    Char,
    Byte,
}

impl QuotedKind {
    /// Classify a literal token's text, or `None` for a numeric literal.
    fn of(literal: &str) -> Option<Self> {
        match (byte(literal, 0), byte(literal, 1)) {
            (b'"' | b'r', _) => Some(Self::Str),
            (b'b', b'"' | b'r') => Some(Self::ByteStr),
            (b'b', b'\'') => Some(Self::Byte),
            (b'c', b'"' | b'r') => Some(Self::CStr),
            (b'\'', _) => Some(Self::Char),
            _ => None,
        }
    }
}

/// Which escapes are allowed, and what they decode to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EscapeMode {
    /// `"..."` / `'...'`: `\x` up to `0x7F`, `\u{...}` allowed.
    Str,
    /// `b"..."` / `b'...'`: ASCII only, `\x` up to `0xFF`, no `\u{...}`.
    Bytes,
    /// `c"..."`: `\x` up to `0xFF` and `\u{...}` (encoded as UTF-8).
    CStr,
}

/// One decoded unit of a quoted literal's contents.
enum Unit {
    Char(char),
    Byte(u8),
}

/// Split the body of a string-like literal (with any `b`/`c` prefix already
/// removed) into `(is_raw, contents, suffix)`.
fn split_string_body(body: &str) -> Option<(bool, &str, &str)> {
    if let Some(raw) = body.strip_prefix('r') {
        let hashes = raw.bytes().take_while(|&b| b == b'#').count();
        let rest = raw[hashes..].strip_prefix('"')?;
        let closing = format!("\"{}", "#".repeat(hashes));
        let end = rest.rfind(&closing)?;
        Some((true, &rest[..end], &rest[end + closing.len()..]))
    } else {
        let rest = body.strip_prefix('"')?;
        let end = rest.rfind('"')?;
        Some((false, &rest[..end], &rest[end + 1..]))
    }
}

/// Split the body of a character-like literal (with any `b` prefix already
/// removed) into `(contents, suffix)`.
fn split_char_body(body: &str) -> Option<(&str, &str)> {
    let rest = body.strip_prefix('\'')?;
    let end = rest.rfind('\'')?;
    Some((&rest[..end], &rest[end + 1..]))
}

/// Read exactly `count` hexadecimal digits.
fn hex_digits(chars: &mut impl Iterator<Item = char>, count: usize) -> Option<u32> {
    let mut value = 0;
    for _ in 0..count {
        value = value * 16 + chars.next()?.to_digit(16)?;
    }
    Some(value)
}

/// Decode a `\u{...}` escape, the `\u` having already been consumed.
fn unicode_escape(chars: &mut impl Iterator<Item = char>) -> core::result::Result<char, String> {
    const ERROR: &str = "Invalid unicode escape, expected `\\u{XXXX}`";
    if chars.next() != Some('{') {
        return Err(ERROR.to_owned());
    }
    let mut value: u32 = 0;
    let mut digits = 0;
    loop {
        match chars.next() {
            Some('}') if digits > 0 => break,
            Some('_') if digits > 0 => {}
            Some(c) if digits < 6 && c.is_ascii_hexdigit() => {
                value = value * 16 + c.to_digit(16).unwrap_or_default();
                digits += 1;
            }
            _ => return Err(ERROR.to_owned()),
        }
    }
    char::from_u32(value).ok_or_else(|| format!("Invalid unicode character escape `\\u{{{value:X}}}`"))
}

/// Decode the contents of a non-raw quoted literal.
fn unescape(
    content: &str,
    mode: EscapeMode,
    allow_continuation: bool,
) -> core::result::Result<Vec<Unit>, String> {
    let ascii = |b: u8| {
        if mode == EscapeMode::Str {
            Unit::Char(char::from(b))
        } else {
            Unit::Byte(b)
        }
    };
    let mut units = Vec::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            units.push(plain_unit(c, mode)?);
            continue;
        }
        let unit = match chars.next() {
            Some('n') => ascii(b'\n'),
            Some('r') => ascii(b'\r'),
            Some('t') => ascii(b'\t'),
            Some('\\') => ascii(b'\\'),
            Some('0') => ascii(b'\0'),
            Some('\'') => ascii(b'\''),
            Some('"') => ascii(b'"'),
            Some('x') => {
                let value = hex_digits(&mut chars, 2)
                    .ok_or_else(|| "Invalid hex escape, expected `\\xHH`".to_owned())?;
                let value = u8::try_from(value).unwrap_or_default();
                if mode == EscapeMode::Str && value > 0x7F {
                    return Err(format!(
                        "Out of range hex escape `\\x{value:02X}`, must be at most `\\x7F`"
                    ));
                }
                ascii(value)
            }
            Some('u') if mode != EscapeMode::Bytes => Unit::Char(unicode_escape(&mut chars)?),
            Some('\n') if allow_continuation => {
                while chars.next_if(|c| matches!(c, ' ' | '\t' | '\n' | '\r')).is_some() {}
                continue;
            }
            Some(other) => return Err(format!("Unknown character escape `\\{}`", other.escape_default())),
            None => return Err("Unterminated escape sequence".to_owned()),
        };
        units.push(unit);
    }
    Ok(units)
}

/// Decode an unescaped character in a quoted literal.
fn plain_unit(c: char, mode: EscapeMode) -> core::result::Result<Unit, String> {
    if mode != EscapeMode::Bytes {
        return Ok(Unit::Char(c));
    }
    u8::try_from(c)
        .ok()
        .filter(u8::is_ascii)
        .map(Unit::Byte)
        .ok_or_else(|| format!("Non-ASCII character `{c}` in byte literal"))
}

/// Decode the contents of a (raw or not) quoted literal.
fn decode(
    is_raw: bool,
    content: &str,
    mode: EscapeMode,
    allow_continuation: bool,
) -> core::result::Result<Vec<Unit>, String> {
    if is_raw {
        content.chars().map(|c| plain_unit(c, mode)).collect()
    } else {
        unescape(content, mode, allow_continuation)
    }
}

/// Encode decoded units as bytes (UTF-8 for characters).
fn units_to_bytes(units: Vec<Unit>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(units.len());
    for unit in units {
        match unit {
            Unit::Byte(b) => bytes.push(b),
            Unit::Char(c) => bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    bytes
}

/// Check that a literal suffix is empty or a valid identifier.
fn check_suffix(suffix: &str) -> core::result::Result<String, String> {
    if suffix.is_empty() || is_identifier_or_keyword(suffix) {
        Ok(suffix.to_owned())
    } else {
        Err(format!("Invalid literal suffix `{suffix}`"))
    }
}

/// Parse the next token as a quoted literal of `kind`, consuming it only on
/// success.
fn parse_quoted<T>(
    input: &mut ParseBuffer,
    kind: QuotedKind,
    expected: &str,
    build: impl FnOnce(&str) -> core::result::Result<T, String>,
) -> Result<(proc_macro::Literal, T)> {
    let Some(literal) = input.peek_literal() else {
        return Err(Diagnostics::new_error_spanned(
            format!("Expected {expected}"),
            input.span(),
        ));
    };
    let text = literal.to_string();
    let span = literal.span();
    if QuotedKind::of(&text) != Some(kind) {
        return Err(Diagnostics::new_error_spanned(
            format!("Expected {expected}"),
            span,
        ));
    }
    let value = build(&text).map_err(|error| Diagnostics::new_error_spanned(error, span))?;
    let Some(token) = input.literal() else {
        unreachable!("a literal was just peeked")
    };
    Ok((token, value))
}

macro_rules! quoted_literal {
    (
        $(#[$meta:meta])*
        $name:ident, $kind:ident, $expected:literal,
        $value_ty:ty, $value_ref:ty, |$value:ident| $as_ref:expr,
        |$text:ident| $build:expr
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $name {
            token: proc_macro::Literal,
            value: $value_ty,
            suffix: String,
        }

        impl $name {
            /// The literal's value, with escapes decoded.
            #[must_use]
            #[allow(clippy::missing_const_for_fn)]
            pub fn value(&self) -> $value_ref {
                let $value = &self.value;
                $as_ref
            }
            /// The literal's suffix, or an empty string if there is none.
            #[must_use]
            pub fn suffix(&self) -> &str {
                &self.suffix
            }
            /// This literal's span.
            #[must_use]
            pub fn span(&self) -> Span {
                self.token.span()
            }
            /// The underlying literal token, exactly as written.
            #[must_use]
            pub const fn token(&self) -> &proc_macro::Literal {
                &self.token
            }
        }

        impl Parse for $name {
            fn parse(input: &mut ParseBuffer) -> Result<Self> {
                let (token, (value, suffix)) =
                    parse_quoted(input, QuotedKind::$kind, $expected, |$text| $build)?;
                Ok(Self { token, value, suffix })
            }
        }

        impl Peek for $name {}

        impl ToTokens for $name {
            fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
                tokens.extend(Some(self.token.clone()));
            }
        }
    };
}

const MALFORMED: &str = "Malformed literal";

quoted_literal! {
    /// A string literal, e.g. `"foo\n"` or `r#"foo"#`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#string-literals>
    LiteralStr, Str, "string literal",
    String, &str, |value| value.as_str(),
    |text| {
        let (is_raw, content, suffix) = split_string_body(text).ok_or(MALFORMED)?;
        let value = decode(is_raw, content, EscapeMode::Str, true)?
            .into_iter()
            .filter_map(|unit| match unit {
                Unit::Char(c) => Some(c),
                Unit::Byte(_) => None,
            })
            .collect();
        Ok((value, check_suffix(suffix)?))
    }
}

quoted_literal! {
    /// A byte string literal, e.g. `b"foo\xFF"` or `br"foo"`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#byte-string-literals>
    LiteralByteStr, ByteStr, "byte string literal",
    Vec<u8>, &[u8], |value| value.as_slice(),
    |text| {
        let (is_raw, content, suffix) = split_string_body(&text[1..]).ok_or(MALFORMED)?;
        let value = units_to_bytes(decode(is_raw, content, EscapeMode::Bytes, true)?);
        Ok((value, check_suffix(suffix)?))
    }
}

quoted_literal! {
    /// A C string literal, e.g. `c"foo"` or `cr"foo"`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#c-string-literals>
    LiteralCStr, CStr, "C string literal",
    CString, &CStr, |value| value.as_c_str(),
    |text| {
        let (is_raw, content, suffix) = split_string_body(&text[1..]).ok_or(MALFORMED)?;
        let value = CString::new(units_to_bytes(decode(is_raw, content, EscapeMode::CStr, true)?))
            .map_err(|_| "C string literals cannot contain a nul byte".to_owned())?;
        Ok((value, check_suffix(suffix)?))
    }
}

quoted_literal! {
    /// A character literal, e.g. `'a'` or `'\u{1F980}'`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#character-literals>
    LiteralChar, Char, "character literal",
    char, char, |value| *value,
    |text| {
        let (content, suffix) = split_char_body(text).ok_or(MALFORMED)?;
        match unescape(content, EscapeMode::Str, false)?.as_slice() {
            [Unit::Char(c)] => Ok((*c, check_suffix(suffix)?)),
            _ => Err("Character literals must contain exactly one character".to_owned()),
        }
    }
}

quoted_literal! {
    /// A byte literal, e.g. `b'a'` or `b'\xFF'`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#byte-literals>
    LiteralByte, Byte, "byte literal",
    u8, u8, |value| *value,
    |text| {
        let (content, suffix) = split_char_body(&text[1..]).ok_or(MALFORMED)?;
        match unescape(content, EscapeMode::Bytes, false)?.as_slice() {
            [Unit::Byte(b)] => Ok((*b, check_suffix(suffix)?)),
            _ => Err("Byte literals must contain exactly one byte".to_owned()),
        }
    }
}

impl Parse for String {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        input.parse::<LiteralStr>().map(|literal| literal.value)
    }
}

impl Peek for String {}

impl Parse for char {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        input.parse::<LiteralChar>().map(|literal| literal.value)
    }
}

impl Peek for char {}

impl Parse for bool {
    #[allow(clippy::cmp_owned)]
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if input.ident_and(|ident| ident.to_string() == "true").is_some() {
            Ok(true)
        } else if input.ident_and(|ident| ident.to_string() == "false").is_some() {
            Ok(false)
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected `true` or `false`",
                input.span(),
            ))
        }
    }
}

impl Peek for bool {}

#[cfg(test)]
mod tests {
    use super::{EscapeMode, Unit, decode, unescape, units_to_bytes};

    fn bytes(content: &str, mode: EscapeMode) -> Result<Vec<u8>, String> {
        unescape(content, mode, true).map(units_to_bytes)
    }

    #[test]
    fn escape_rules() {
        assert_eq!(bytes(r"\x7F", EscapeMode::Str).unwrap(), b"\x7F");
        assert!(bytes(r"\x80", EscapeMode::Str).is_err());
        assert_eq!(bytes(r"\x80", EscapeMode::Bytes).unwrap(), b"\x80");
        assert!(bytes(r"\u{e9}", EscapeMode::Bytes).is_err());
        assert_eq!(bytes(r"\u{e9}", EscapeMode::CStr).unwrap(), "\u{e9}".as_bytes());
        assert!(bytes("\u{e9}", EscapeMode::Bytes).is_err());
        assert!(decode(true, "\u{e9}", EscapeMode::Bytes, true).is_err());
        assert!(bytes(r"\q", EscapeMode::Str).is_err());
        assert!(bytes(r"\x4", EscapeMode::Str).is_err());
        assert!(bytes(r"\u{}", EscapeMode::Str).is_err());
        assert!(bytes(r"\u{1234567}", EscapeMode::Str).is_err());
        assert!(bytes(r"\u{D800}", EscapeMode::Str).is_err());
        assert!(bytes("\\", EscapeMode::Str).is_err());
        assert!(unescape("\\\n", EscapeMode::Str, false).is_err());
        assert!(matches!(
            unescape(r"\n", EscapeMode::Bytes, false).unwrap().as_slice(),
            [Unit::Byte(b'\n')]
        ));
    }
}
