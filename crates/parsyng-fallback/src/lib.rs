//! A pure-Rust implementation of the token types of the compiler's
//! [`proc_macro`](https://doc.rust-lang.org/proc_macro/) crate.
//!
//! `parsyng` uses the compiler's `proc_macro` inside macros, where it is the
//! only option. That crate panics everywhere else (unit tests, `build.rs`,
//! benchmarks), so the `fallback` feature of `parsyng` swaps it for this
//! one. The API mirrors `proc_macro`, so the same code compiles against
//! both.
//!
//! Unlike `proc_macro2`, this crate never forwards to the compiler: there is
//! no runtime check and no conversion, which keeps it small and fast.
//! Identifiers and literals share their text (`Rc<str>`, or a `&'static str`
//! for tokens built by `quote!`), so cloning a token never allocates, and a
//! [`TokenStream`] is a shared vector, so cloning one is O(1).
//!
//! A [`Span`] is the byte range of a token in the string it was lexed from
//! (see [`Span::byte_range`]); tokens built in code are spanned at `0..0`.
//!
//! A [`TokenStream`] still converts from and into the compiler's
//! `proc_macro::TokenStream`, by printing and re-lexing it (spans are lost):
//! this only happens when a proc-macro crate ends up built with `parsyng`'s
//! `fallback` feature (e.g. through Cargo's feature unification), so that it
//! keeps compiling and working, just slower.

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
// The public API mirrors the compiler's `proc_macro`, where nothing is
// `const`: code written against both must not see a difference.
#![allow(clippy::missing_const_for_fn)]

extern crate proc_macro;

mod lex;

use core::fmt;
use core::ops::Range;
use core::str::FromStr;
use std::ffi::CStr;
use std::fmt::Write as _;
use std::rc::Rc;

/// The text of an identifier or literal: shared, so clones never allocate.
#[derive(Clone)]
enum Sym {
    Static(&'static str),
    Shared(Rc<str>),
}

impl Sym {
    fn as_str(&self) -> &str {
        match self {
            Self::Static(text) => text,
            Self::Shared(text) => text,
        }
    }
}

impl From<String> for Sym {
    fn from(text: String) -> Self {
        Self::Shared(text.into())
    }
}

/// A region of source code: the byte range of a token in the string it was
/// lexed from.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    lo: u32,
    hi: u32,
}

impl Span {
    /// The span of tokens built in code: `0..0`.
    #[must_use]
    pub fn call_site() -> Self {
        Self { lo: 0, hi: 0 }
    }

    /// Same as [`Span::call_site`]: there is no hygiene outside the compiler.
    #[must_use]
    pub fn mixed_site() -> Self {
        Self::call_site()
    }

    pub(crate) fn new(range: Range<usize>) -> Self {
        let clamp = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Self {
            lo: clamp(range.start),
            hi: clamp(range.end),
        }
    }

    /// The byte range this span covers in the lexed string.
    #[must_use]
    pub fn byte_range(&self) -> Range<usize> {
        self.lo as usize..self.hi as usize
    }

    /// A span covering both `self` and `other`.
    #[must_use]
    #[allow(clippy::unnecessary_wraps)]
    pub fn join(&self, other: Self) -> Option<Self> {
        Some(Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        })
    }

    /// `self`, since there is no hygiene outside the compiler.
    #[must_use]
    pub fn resolved_at(&self, _other: Self) -> Self {
        *self
    }

    /// `other`, since there is no hygiene outside the compiler.
    #[must_use]
    pub fn located_at(&self, other: Self) -> Self {
        other
    }

    /// Always `None`: spans don't keep their source text.
    #[must_use]
    pub fn source_text(&self) -> Option<String> {
        None
    }

    /// The first byte of this span.
    const fn first_byte(self) -> Self {
        Self {
            lo: self.lo,
            hi: if self.lo < self.hi { self.lo + 1 } else { self.hi },
        }
    }

    /// The last byte of this span.
    const fn last_byte(self) -> Self {
        Self {
            lo: if self.lo < self.hi { self.hi - 1 } else { self.lo },
            hi: self.hi,
        }
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bytes({}..{})", self.lo, self.hi)
    }
}

/// How a [`Group`] is delimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Delimiter {
    /// `( ... )`
    Parenthesis,
    /// `{ ... }`
    Brace,
    /// `[ ... ]`
    Bracket,
    /// An invisible delimiter.
    None,
}

/// Whether a [`Punct`] is immediately followed by another one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spacing {
    /// Followed by another punctuation character, forming a multi-character
    /// operator such as `+=`.
    Joint,
    /// Followed by something else.
    Alone,
}

/// A single token, or a delimited sequence of tokens.
#[derive(Clone)]
pub enum TokenTree {
    /// A delimited token stream.
    Group(Group),
    /// An identifier or keyword.
    Ident(Ident),
    /// A punctuation character.
    Punct(Punct),
    /// A literal.
    Literal(Literal),
}

impl TokenTree {
    /// This token's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Group(group) => group.span(),
            Self::Ident(ident) => ident.span(),
            Self::Punct(punct) => punct.span(),
            Self::Literal(literal) => literal.span(),
        }
    }

    /// Set this token's span.
    pub fn set_span(&mut self, span: Span) {
        match self {
            Self::Group(group) => group.set_span(span),
            Self::Ident(ident) => ident.set_span(span),
            Self::Punct(punct) => punct.set_span(span),
            Self::Literal(literal) => literal.set_span(span),
        }
    }
}

impl From<Group> for TokenTree {
    fn from(group: Group) -> Self {
        Self::Group(group)
    }
}
impl From<Ident> for TokenTree {
    fn from(ident: Ident) -> Self {
        Self::Ident(ident)
    }
}
impl From<Punct> for TokenTree {
    fn from(punct: Punct) -> Self {
        Self::Punct(punct)
    }
}
impl From<Literal> for TokenTree {
    fn from(literal: Literal) -> Self {
        Self::Literal(literal)
    }
}

impl fmt::Display for TokenTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Group(group) => fmt::Display::fmt(group, f),
            Self::Ident(ident) => fmt::Display::fmt(ident, f),
            Self::Punct(punct) => fmt::Display::fmt(punct, f),
            Self::Literal(literal) => fmt::Display::fmt(literal, f),
        }
    }
}

impl fmt::Debug for TokenTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Group(group) => fmt::Debug::fmt(group, f),
            Self::Ident(ident) => fmt::Debug::fmt(ident, f),
            Self::Punct(punct) => fmt::Debug::fmt(punct, f),
            Self::Literal(literal) => fmt::Debug::fmt(literal, f),
        }
    }
}

/// A sequence of token trees. Cloning one is O(1): the trees are shared
/// until one of the copies is modified.
#[derive(Clone, Default)]
pub struct TokenStream {
    /// `None` for an empty stream, so that [`TokenStream::new`] never
    /// allocates.
    inner: Option<Rc<Vec<TokenTree>>>,
}

impl TokenStream {
    /// An empty stream.
    #[must_use]
    pub fn new() -> Self {
        Self { inner: None }
    }

    /// Whether this stream has no tokens.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.as_ref().is_none_or(|trees| trees.is_empty())
    }

    fn from_vec(trees: Vec<TokenTree>) -> Self {
        Self {
            inner: (!trees.is_empty()).then(|| Rc::new(trees)),
        }
    }

    fn trees(&self) -> &[TokenTree] {
        self.inner.as_deref().map_or(&[], Vec::as_slice)
    }

    fn trees_mut(&mut self) -> &mut Vec<TokenTree> {
        Rc::make_mut(self.inner.get_or_insert_with(Rc::default))
    }
}

/// Converts the compiler's tokens by printing and re-lexing them: their
/// spans are lost.
impl From<proc_macro::TokenStream> for TokenStream {
    fn from(stream: proc_macro::TokenStream) -> Self {
        stream
            .to_string()
            .parse()
            .expect("the compiler's tokens always lex")
    }
}

/// Converts into the compiler's tokens by printing and re-lexing them: their
/// spans are lost. Only works inside a procedural macro.
impl From<TokenStream> for proc_macro::TokenStream {
    fn from(stream: TokenStream) -> Self {
        stream
            .to_string()
            .parse()
            .expect("printed tokens always lex")
    }
}

impl From<TokenTree> for TokenStream {
    fn from(tree: TokenTree) -> Self {
        Self::from_vec(vec![tree])
    }
}

/// Like the compiler's `proc_macro`, any single token (or tree) can be
/// appended.
impl<T: Into<TokenTree>> Extend<T> for TokenStream {
    fn extend<I: IntoIterator<Item = T>>(&mut self, trees: I) {
        self.trees_mut().extend(trees.into_iter().map(Into::into));
    }
}

impl Extend<Self> for TokenStream {
    fn extend<I: IntoIterator<Item = Self>>(&mut self, streams: I) {
        for stream in streams {
            if self.is_empty() {
                *self = stream;
            } else if !stream.is_empty() {
                self.trees_mut().extend(stream);
            }
        }
    }
}

impl FromIterator<TokenTree> for TokenStream {
    fn from_iter<I: IntoIterator<Item = TokenTree>>(trees: I) -> Self {
        Self::from_vec(trees.into_iter().collect())
    }
}

impl FromIterator<Self> for TokenStream {
    fn from_iter<I: IntoIterator<Item = Self>>(streams: I) -> Self {
        let mut stream = Self::new();
        stream.extend(streams);
        stream
    }
}

impl IntoIterator for TokenStream {
    type Item = TokenTree;
    type IntoIter = token_stream::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        token_stream::IntoIter::new(self.inner)
    }
}

impl FromStr for TokenStream {
    type Err = LexError;

    fn from_str(src: &str) -> Result<Self, LexError> {
        lex::lex(src)
    }
}

impl fmt::Display for TokenStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut joint = false;
        for (i, tree) in self.trees().iter().enumerate() {
            if i != 0 && !joint {
                f.write_str(" ")?;
            }
            joint = matches!(tree, TokenTree::Punct(punct) if punct.spacing == Spacing::Joint);
            fmt::Display::fmt(tree, f)?;
        }
        Ok(())
    }
}

impl fmt::Debug for TokenStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenStream ")?;
        f.debug_list().entries(self.trees()).finish()
    }
}

/// Iteration over a [`TokenStream`].
pub mod token_stream {
    use std::rc::Rc;

    use crate::TokenTree;

    /// An iterator over the trees of a [`TokenStream`](crate::TokenStream).
    /// It moves them out if the stream isn't shared, and clones them
    /// (cheaply) otherwise.
    #[derive(Clone)]
    pub struct IntoIter(Inner);

    #[derive(Clone)]
    enum Inner {
        Owned(std::vec::IntoIter<TokenTree>),
        Shared(Rc<Vec<TokenTree>>, usize),
    }

    impl IntoIter {
        pub(crate) fn new(trees: Option<Rc<Vec<TokenTree>>>) -> Self {
            Self(trees.map_or_else(
                || Inner::Owned(Vec::new().into_iter()),
                |trees| match Rc::try_unwrap(trees) {
                    Ok(trees) => Inner::Owned(trees.into_iter()),
                    Err(trees) => Inner::Shared(trees, 0),
                },
            ))
        }
    }

    impl Iterator for IntoIter {
        type Item = TokenTree;

        fn next(&mut self) -> Option<TokenTree> {
            match &mut self.0 {
                Inner::Owned(trees) => trees.next(),
                Inner::Shared(trees, pos) => {
                    let tree = trees.get(*pos)?.clone();
                    *pos += 1;
                    Some(tree)
                }
            }
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            let len = match &self.0 {
                Inner::Owned(trees) => trees.len(),
                Inner::Shared(trees, pos) => trees.len() - *pos,
            };
            (len, Some(len))
        }
    }

    impl ExactSizeIterator for IntoIter {}
}

/// A delimited token stream.
#[derive(Clone)]
pub struct Group {
    delimiter: Delimiter,
    stream: TokenStream,
    span: Span,
}

impl Group {
    /// A group of `stream` delimited by `delimiter`, spanned at
    /// [`Span::call_site`].
    #[must_use]
    pub fn new(delimiter: Delimiter, stream: TokenStream) -> Self {
        Self {
            delimiter,
            stream,
            span: Span::call_site(),
        }
    }

    /// This group's delimiter.
    #[must_use]
    pub fn delimiter(&self) -> Delimiter {
        self.delimiter
    }

    /// This group's contents, without the delimiters. O(1).
    #[must_use]
    pub fn stream(&self) -> TokenStream {
        self.stream.clone()
    }

    /// The span of the whole group, delimiters included.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }

    /// The span of the opening delimiter.
    #[must_use]
    pub fn span_open(&self) -> Span {
        self.span.first_byte()
    }

    /// The span of the closing delimiter.
    #[must_use]
    pub fn span_close(&self) -> Span {
        self.span.last_byte()
    }

    /// Set the span of the whole group.
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (open, close) = match self.delimiter {
            Delimiter::Parenthesis => ("(", ")"),
            Delimiter::Brace => ("{ ", "}"),
            Delimiter::Bracket => ("[", "]"),
            Delimiter::None => ("", ""),
        };
        f.write_str(open)?;
        fmt::Display::fmt(&self.stream, f)?;
        if self.delimiter == Delimiter::Brace && !self.stream.is_empty() {
            f.write_str(" ")?;
        }
        f.write_str(close)
    }
}

impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Group")
            .field("delimiter", &self.delimiter)
            .field("stream", &self.stream)
            .field("span", &self.span)
            .finish()
    }
}

/// An identifier or keyword.
#[derive(Clone)]
pub struct Ident {
    sym: Sym,
    raw: bool,
    span: Span,
}

/// Whether `ch` can start an identifier.
pub(crate) fn is_ident_start(ch: char) -> bool {
    ch == '_' || unicode_ident::is_xid_start(ch)
}

/// Whether `ch` can continue an identifier.
pub(crate) fn is_ident_continue(ch: char) -> bool {
    unicode_ident::is_xid_continue(ch)
}

fn is_valid_ident(string: &str) -> bool {
    let mut chars = string.chars();
    chars.next().is_some_and(is_ident_start) && chars.all(is_ident_continue)
}

impl Ident {
    /// An identifier or keyword. A `r#` prefix makes it raw.
    ///
    /// # Panics
    /// Panics if `string` is not a valid identifier.
    #[must_use]
    pub fn new(string: &str, span: Span) -> Self {
        if let Some(raw) = string.strip_prefix("r#") {
            return Self::new_raw(raw, span);
        }
        assert!(is_valid_ident(string), "`{string:?}` is not a valid identifier");
        Self {
            sym: Sym::Shared(string.into()),
            raw: false,
            span,
        }
    }

    /// A raw identifier (`r#string`).
    ///
    /// # Panics
    /// Panics if `string` is not a valid raw identifier.
    #[must_use]
    pub fn new_raw(string: &str, span: Span) -> Self {
        assert!(
            is_valid_ident(string) && !matches!(string, "_" | "super" | "self" | "Self" | "crate"),
            "`{string:?}` is not a valid raw identifier"
        );
        Self {
            sym: Sym::Shared(string.into()),
            raw: true,
            span,
        }
    }

    /// An identifier whose text is known to be valid, without allocating:
    /// used by `quote!`'s expansion.
    #[doc(hidden)]
    #[must_use]
    pub fn new_static_unchecked(string: &'static str, raw: bool, span: Span) -> Self {
        Self {
            sym: Sym::Static(string),
            raw,
            span,
        }
    }

    pub(crate) fn new_lexed(string: &str, raw: bool, span: Span) -> Self {
        Self {
            sym: Sym::Shared(string.into()),
            raw,
            span,
        }
    }

    /// This identifier's span.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }

    /// Set this identifier's span.
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.raw {
            f.write_str("r#")?;
        }
        f.write_str(self.sym.as_str())
    }
}

impl fmt::Debug for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ident({self})")
    }
}

/// A punctuation character.
#[derive(Clone)]
pub struct Punct {
    ch: char,
    spacing: Spacing,
    span: Span,
}

/// Every character a [`Punct`] can hold.
pub(crate) const PUNCT_CHARS: &str = "=<>!~+-*/%^&|@.,;:#$?'";

impl Punct {
    /// A punctuation character, spanned at [`Span::call_site`].
    ///
    /// # Panics
    /// Panics if `ch` is not a punctuation character.
    #[must_use]
    pub fn new(ch: char, spacing: Spacing) -> Self {
        assert!(PUNCT_CHARS.contains(ch), "unsupported character `{ch:?}`");
        Self {
            ch,
            spacing,
            span: Span::call_site(),
        }
    }

    /// This punctuation's character.
    #[must_use]
    pub fn as_char(&self) -> char {
        self.ch
    }

    /// Whether another punctuation character immediately follows.
    #[must_use]
    pub fn spacing(&self) -> Spacing {
        self.spacing
    }

    /// This punctuation's span.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }

    /// Set this punctuation's span.
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}

impl fmt::Display for Punct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char(self.ch)
    }
}

impl fmt::Debug for Punct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Punct")
            .field("char", &self.ch)
            .field("spacing", &self.spacing)
            .field("span", &self.span)
            .finish()
    }
}

/// A literal: string, byte string, C string, character, byte or number,
/// kept as its source text.
#[derive(Clone)]
pub struct Literal {
    repr: Sym,
    span: Span,
}

macro_rules! suffixed_int_literals {
    ($($name:ident => $kind:ident,)*) => {$(
        #[doc = concat!("A `", stringify!($kind), "` literal with its suffix, e.g. `1", stringify!($kind), "`.")]
        #[must_use]
        pub fn $name(n: $kind) -> Self {
            Self::from_repr(format!(concat!("{}", stringify!($kind)), n))
        }
    )*};
}

macro_rules! unsuffixed_int_literals {
    ($($name:ident => $kind:ident,)*) => {$(
        #[doc = concat!("A `", stringify!($kind), "` literal without suffix, e.g. `1`.")]
        #[must_use]
        pub fn $name(n: $kind) -> Self {
            Self::from_repr(n.to_string())
        }
    )*};
}

/// Escape `string` into `repr` as the inside of a string literal.
fn escape_utf8(string: &str, repr: &mut String) {
    let mut chars = string.chars();
    while let Some(ch) = chars.next() {
        if ch == '\0' {
            // `\0` followed by a digit would read as an octal-looking escape.
            repr.push_str(if chars.as_str().starts_with(|next: char| next.is_ascii_digit()) {
                "\\x00"
            } else {
                "\\0"
            });
        } else if ch == '\'' {
            repr.push(ch);
        } else {
            repr.extend(ch.escape_debug());
        }
    }
}

/// Escape `bytes` into `repr` as the inside of a byte string literal.
fn escape_bytes(bytes: &[u8], repr: &mut String) {
    for (i, &byte) in bytes.iter().enumerate() {
        match byte {
            b'\0' if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) => repr.push_str("\\x00"),
            b'\0' => repr.push_str("\\0"),
            b'\t' => repr.push_str("\\t"),
            b'\n' => repr.push_str("\\n"),
            b'\r' => repr.push_str("\\r"),
            b'"' => repr.push_str("\\\""),
            b'\\' => repr.push_str("\\\\"),
            b'\x20'..=b'\x7E' => repr.push(char::from(byte)),
            _ => {
                let _ = write!(repr, "\\x{byte:02X}");
            }
        }
    }
}

impl Literal {
    fn from_repr(repr: String) -> Self {
        Self {
            repr: repr.into(),
            span: Span::call_site(),
        }
    }

    /// A literal whose source text is known to be valid, without
    /// allocating: used by `quote!`'s expansion.
    #[doc(hidden)]
    #[must_use]
    pub fn new_static_unchecked(repr: &'static str, span: Span) -> Self {
        Self {
            repr: Sym::Static(repr),
            span,
        }
    }

    pub(crate) fn new_lexed(repr: &str, span: Span) -> Self {
        Self {
            repr: Sym::Shared(repr.into()),
            span,
        }
    }

    suffixed_int_literals! {
        u8_suffixed => u8, u16_suffixed => u16, u32_suffixed => u32, u64_suffixed => u64,
        u128_suffixed => u128, usize_suffixed => usize, i8_suffixed => i8, i16_suffixed => i16,
        i32_suffixed => i32, i64_suffixed => i64, i128_suffixed => i128, isize_suffixed => isize,
    }

    unsuffixed_int_literals! {
        u8_unsuffixed => u8, u16_unsuffixed => u16, u32_unsuffixed => u32, u64_unsuffixed => u64,
        u128_unsuffixed => u128, usize_unsuffixed => usize, i8_unsuffixed => i8,
        i16_unsuffixed => i16, i32_unsuffixed => i32, i64_unsuffixed => i64,
        i128_unsuffixed => i128, isize_unsuffixed => isize,
    }

    /// An `f32` literal without suffix, e.g. `1.5`.
    ///
    /// # Panics
    /// Panics if `f` is not finite.
    #[must_use]
    pub fn f32_unsuffixed(f: f32) -> Self {
        assert!(f.is_finite(), "invalid float literal {f}");
        Self::from_repr(float_repr(f.to_string()))
    }

    /// An `f32` literal with its suffix, e.g. `1.5f32`.
    ///
    /// # Panics
    /// Panics if `f` is not finite.
    #[must_use]
    pub fn f32_suffixed(f: f32) -> Self {
        assert!(f.is_finite(), "invalid float literal {f}");
        Self::from_repr(format!("{f}f32"))
    }

    /// An `f64` literal without suffix, e.g. `1.5`.
    ///
    /// # Panics
    /// Panics if `f` is not finite.
    #[must_use]
    pub fn f64_unsuffixed(f: f64) -> Self {
        assert!(f.is_finite(), "invalid float literal {f}");
        Self::from_repr(float_repr(f.to_string()))
    }

    /// An `f64` literal with its suffix, e.g. `1.5f64`.
    ///
    /// # Panics
    /// Panics if `f` is not finite.
    #[must_use]
    pub fn f64_suffixed(f: f64) -> Self {
        assert!(f.is_finite(), "invalid float literal {f}");
        Self::from_repr(format!("{f}f64"))
    }

    /// A string literal.
    #[must_use]
    pub fn string(string: &str) -> Self {
        let mut repr = String::with_capacity(string.len() + 2);
        repr.push('"');
        escape_utf8(string, &mut repr);
        repr.push('"');
        Self::from_repr(repr)
    }

    /// A character literal.
    #[must_use]
    pub fn character(ch: char) -> Self {
        let mut repr = String::from("'");
        if ch == '"' {
            repr.push(ch);
        } else {
            repr.extend(ch.escape_debug());
        }
        repr.push('\'');
        Self::from_repr(repr)
    }

    /// A byte character literal, e.g. `b'a'`.
    #[must_use]
    pub fn byte_character(byte: u8) -> Self {
        let mut repr = String::from("b'");
        match byte {
            b'\'' => repr.push_str("\\'"),
            b'"' => repr.push('"'),
            _ => escape_bytes(&[byte], &mut repr),
        }
        repr.push('\'');
        Self::from_repr(repr)
    }

    /// A byte string literal, e.g. `b"abc"`.
    #[must_use]
    pub fn byte_string(bytes: &[u8]) -> Self {
        let mut repr = String::from("b\"");
        escape_bytes(bytes, &mut repr);
        repr.push('"');
        Self::from_repr(repr)
    }

    /// A C string literal, e.g. `c"abc"`.
    #[must_use]
    pub fn c_string(string: &CStr) -> Self {
        let mut repr = String::from("c\"");
        let mut bytes = string.to_bytes();
        while !bytes.is_empty() {
            match core::str::from_utf8(bytes) {
                Ok(valid) => {
                    escape_utf8(valid, &mut repr);
                    break;
                }
                Err(error) => {
                    let (valid, rest) = bytes.split_at(error.valid_up_to());
                    escape_utf8(core::str::from_utf8(valid).unwrap_or_default(), &mut repr);
                    let _ = write!(repr, "\\x{:02X}", rest[0]);
                    bytes = &rest[1..];
                }
            }
        }
        repr.push('"');
        Self::from_repr(repr)
    }

    /// This literal's span.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }

    /// Set this literal's span.
    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }
}

/// `1` -> `1.0`, so that an unsuffixed float stays a float.
fn float_repr(mut repr: String) -> String {
    if !repr.contains(['.', 'e', 'E']) {
        repr.push_str(".0");
    }
    repr
}

impl FromStr for Literal {
    type Err = LexError;

    /// Lex a single literal, optionally preceded by `-`.
    fn from_str(src: &str) -> Result<Self, LexError> {
        lex::lex_literal(src)
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.repr.as_str())
    }
}

impl fmt::Debug for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Literal({self})")
    }
}

/// The error returned when a string can't be lexed into tokens.
#[derive(Clone, Copy)]
pub struct LexError {
    span: Span,
}

impl LexError {
    /// Where lexing failed.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot parse string into token stream (at byte {})", self.span.lo)
    }
}

impl fmt::Debug for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LexError({:?})", self.span)
    }
}

impl std::error::Error for LexError {}
