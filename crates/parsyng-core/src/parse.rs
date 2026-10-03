//! The [`ParseBuffer`] cursor and the
//! [`Parse`]/[`Peek`] traits it
//! drives.
//!
//! [`ParseBuffer`] is a cursor over a [`TokenStream`], flattened once into a
//! shared buffer, and gives every [`Parse`] implementation a uniform
//! way to consume tokens, try alternatives without committing on failure
//! ([`ParseBuffer::try_parse`](crate::parse::ParseBuffer::try_parse)), and
//! report errors with a useful [`Span`]. Most code
//! using `parsyng` only ever
//! touches [`ParseBuffer::new`](crate::parse::ParseBuffer::new) and
//! [`ParseBuffer::parse`](crate::parse::ParseBuffer::parse); the `peek_*`
//! and `*_and` methods exist for [`Parse`]
//! implementations themselves to decide between grammar alternatives before
//! committing to one.

use std::rc::Rc;

use crate::ToTokens;

use crate::ast::tokens::{NOT_A_KEYWORD, keyword_index};
use crate::error::Diagnostics;
use crate::{
    error::Result,
    proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree},
};

/// What the parser needs to know about a token, computed once when the
/// buffer is built so that lookahead never has to stringify a token again.
#[derive(Clone, Copy)]
enum Kind {
    /// The text is `Buffer::text[start..end]`.
    Ident { start: u32, end: u32, keyword: u8 },
    Punct { ch: char, joint: bool },
    /// The text is `Buffer::text[start..end]`.
    Literal { start: u32, end: u32 },
    /// `end` is the index just past the group's (flattened) contents.
    Group { end: u32, delimiter: Delimiter },
}

struct Entry {
    tt: TokenTree,
    kind: Kind,
}

/// A flattened token stream, shared by every cursor over it.
struct Buffer {
    entries: Vec<Entry>,
    /// The text of every identifier and literal, back to back: one
    /// allocation for the whole stream instead of one per token.
    text: String,
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("token stream too large")
}

/// Append the `Display` of `token` to `text`, returning its range.
fn push_text(text: &mut String, token: &impl core::fmt::Display) -> (u32, u32) {
    use core::fmt::Write as _;
    let start = text.len();
    write!(text, "{token}").expect("writing to a String cannot fail");
    (to_u32(start), to_u32(text.len()))
}

/// Flatten `stream` into `out`: every group is followed by its contents.
fn flatten(stream: TokenStream, out: &mut Vec<Entry>, text: &mut String) {
    let stream = stream.into_iter();
    out.reserve(stream.size_hint().0);
    for tt in stream {
        let kind = match &tt {
            TokenTree::Group(group) => {
                let index = out.len();
                let inner = group.stream();
                let delimiter = group.delimiter();
                out.push(Entry {
                    tt,
                    kind: Kind::Group { end: 0, delimiter },
                });
                flatten(inner, out, text);
                out[index].kind = Kind::Group {
                    end: to_u32(out.len()),
                    delimiter,
                };
                continue;
            }
            TokenTree::Ident(ident) => {
                let (start, end) = push_text(text, ident);
                let keyword = keyword_index(&text[start as usize..]);
                Kind::Ident {
                    start,
                    end,
                    keyword,
                }
            }
            TokenTree::Punct(punct) => Kind::Punct {
                ch: punct.as_char(),
                joint: punct.spacing() == Spacing::Joint,
            },
            TokenTree::Literal(literal) => {
                let (start, end) = push_text(text, literal);
                Kind::Literal { start, end }
            }
        };
        out.push(Entry { tt, kind });
    }
}

/// A cursor over a [`TokenStream`].
///
/// `ParseBuffer` is the input type every [`Parse::parse`] implementation
/// receives. The stream is flattened once into a shared, immutable buffer,
/// and a `ParseBuffer` is only a position in it: [`Clone`] is O(1) and never
/// allocates, and backtracking is free: [`try_parse`](Self::try_parse) and
/// [`try_advance`](Self::try_advance) remember the position, attempt a parse,
/// and rewind on failure, leaving `self` untouched.
///
/// # Example
///
/// ```no_run
/// use parsyng_core::ast::item::ItemStruct;
/// use parsyng_core::parse::ParseBuffer;
///
/// fn parse_struct(tokens: parsyng_core::proc_macro::TokenStream) {
///     let mut input = ParseBuffer::new(tokens);
///     let item: ItemStruct = input.parse().expect("expected a struct");
/// }
/// ```
#[derive(Clone)]
pub struct ParseBuffer {
    buffer: Rc<Buffer>,
    pos: u32,
    end: u32,
    last_span: Span,
}

impl ParseBuffer {
    /// Create a new buffer from a token stream.
    #[must_use]
    pub fn new(inner: crate::proc_macro::TokenStream) -> Self {
        let mut entries = Vec::new();
        let mut text = String::new();
        flatten(inner, &mut entries, &mut text);
        let end = to_u32(entries.len());
        let last_span = entries.first().map_or_else(Span::call_site, |e| e.tt.span());
        Self {
            buffer: Rc::new(Buffer { entries, text }),
            pos: 0,
            end,
            last_span,
        }
    }

    #[inline]
    fn entry(&self) -> Option<&Entry> {
        if self.pos < self.end {
            Some(&self.buffer.entries[self.pos as usize])
        } else {
            None
        }
    }

    #[inline]
    fn nth_entry(&self, n: u32) -> Option<&Entry> {
        let pos = self.pos + n;
        if pos < self.end {
            Some(&self.buffer.entries[pos as usize])
        } else {
            None
        }
    }

    /// Move past the current token (a whole group, for a group) and return it.
    #[inline]
    fn bump(&mut self) -> Option<&Entry> {
        let pos = self.pos;
        if pos >= self.end {
            return None;
        }
        let entry = &self.buffer.entries[pos as usize];
        self.pos = match entry.kind {
            Kind::Group { end, .. } => end,
            _ => pos + 1,
        };
        self.last_span = entry.tt.span();
        Some(entry)
    }

    /// Skip the next token tree (a whole group, for a group) without
    /// cloning it. Returns `false` if the buffer is empty.
    pub fn bump_token(&mut self) -> bool {
        self.bump().is_some()
    }

    /// Span of the next token, or the last consumed token if the stream is empty.
    #[must_use]
    pub fn span(&self) -> Span {
        self.entry().map_or(self.last_span, |e| e.tt.span())
    }

    /// Return `true` when no tokens remain.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pos >= self.end
    }

    /// Inspect the next token without consuming it.
    #[must_use]
    pub fn peek(&self) -> Option<&TokenTree> {
        self.entry().map(|e| &e.tt)
    }

    /// Inspect the next token as a group without consuming it.
    #[must_use]
    pub fn peek_group(&self) -> Option<&Group> {
        match self.peek() {
            Some(TokenTree::Group(group)) => Some(group),
            _ => None,
        }
    }
    /// Inspect the next token as an identifier without consuming it.
    #[must_use]
    pub fn peek_ident(&self) -> Option<&crate::proc_macro::Ident> {
        match self.peek() {
            Some(TokenTree::Ident(ident)) => Some(ident),
            _ => None,
        }
    }
    /// Inspect the next token as punctuation without consuming it.
    #[must_use]
    pub fn peek_punct(&self) -> Option<&crate::proc_macro::Punct> {
        match self.peek() {
            Some(TokenTree::Punct(punct)) => Some(punct),
            _ => None,
        }
    }
    /// Inspect the next token as a literal without consuming it.
    #[must_use]
    pub fn peek_literal(&self) -> Option<&crate::proc_macro::Literal> {
        match self.peek() {
            Some(TokenTree::Literal(literal)) => Some(literal),
            _ => None,
        }
    }

    /// The text of the next token if it is an identifier (`r#` included for
    /// raw identifiers), without consuming it or allocating.
    #[must_use]
    pub fn peek_ident_str(&self) -> Option<&str> {
        self.nth_ident_str(0)
    }
    /// The text of the `n`th next top-level-or-nested token if it is an
    /// identifier. `n` counts flattened tokens, so it only makes sense when
    /// none of the skipped tokens is a group.
    #[must_use]
    pub fn nth_ident_str(&self, n: u32) -> Option<&str> {
        match self.nth_entry(n)?.kind {
            Kind::Ident { start, end, .. } => Some(&self.buffer.text[start as usize..end as usize]),
            _ => None,
        }
    }
    /// The text of the next token if it is a literal, without consuming it
    /// or allocating.
    #[must_use]
    pub fn peek_literal_str(&self) -> Option<&str> {
        match self.entry()?.kind {
            Kind::Literal { start, end } => Some(&self.buffer.text[start as usize..end as usize]),
            _ => None,
        }
    }
    /// The index of the keyword the next identifier spells (as used by
    /// [`RustKeyword`](crate::ast::tokens::RustKeyword)), if any.
    #[must_use]
    pub fn peek_keyword(&self) -> Option<u8> {
        match self.entry()?.kind {
            Kind::Ident { keyword, .. } if keyword != NOT_A_KEYWORD => Some(keyword),
            _ => None,
        }
    }
    /// The character and spacing (`true` for joint) of the `n`th next token
    /// if it is a punctuation, without consuming anything. `n` counts
    /// flattened tokens, so it only makes sense when none of the skipped
    /// tokens is a group.
    #[must_use]
    pub fn nth_punct_char(&self, n: u32) -> Option<(char, bool)> {
        match self.nth_entry(n)?.kind {
            Kind::Punct { ch, joint } => Some((ch, joint)),
            _ => None,
        }
    }
    /// The character and spacing (`true` for joint) of the next token if it
    /// is a punctuation, without consuming it.
    #[must_use]
    pub fn peek_punct_char(&self) -> Option<(char, bool)> {
        self.nth_punct_char(0)
    }
    /// The delimiter of the `n`th next token if it is a group. `n` counts
    /// flattened tokens, so it only makes sense when none of the skipped
    /// tokens is a group.
    #[must_use]
    pub fn nth_delimiter(&self, n: u32) -> Option<Delimiter> {
        match self.nth_entry(n)?.kind {
            Kind::Group { delimiter, .. } => Some(delimiter),
            _ => None,
        }
    }
    /// The delimiter of the next token if it is a group, without consuming it.
    #[must_use]
    pub fn peek_delimiter(&self) -> Option<Delimiter> {
        match self.entry()?.kind {
            Kind::Group { delimiter, .. } => Some(delimiter),
            _ => None,
        }
    }

    /// Consume and return the next group token.
    pub fn group(&mut self) -> Option<Group> {
        match self.entry()?.tt {
            TokenTree::Group(_) => match &self.bump()?.tt {
                TokenTree::Group(group) => Some(group.clone()),
                _ => None,
            },
            _ => None,
        }
    }
    /// Consume the next group token, returning it along with a buffer over
    /// its contents (sharing this buffer, so this is O(1)).
    pub fn group_contents(&mut self) -> Option<(Group, Self)> {
        let pos = self.pos;
        let Kind::Group { end, .. } = self.entry()?.kind else {
            return None;
        };
        let group = self.group()?;
        let contents = Self {
            buffer: Rc::clone(&self.buffer),
            pos: pos + 1,
            end,
            last_span: group.span_close(),
        };
        Some((group, contents))
    }
    /// Consume the next group token if it has the given delimiter, returning
    /// it along with a buffer over its contents.
    pub fn delimited(&mut self, delimiter: Delimiter) -> Option<(Group, Self)> {
        if self.peek_delimiter()? == delimiter {
            self.group_contents()
        } else {
            None
        }
    }
    /// Consume and return the next identifier token.
    pub fn ident(&mut self) -> Option<crate::proc_macro::Ident> {
        self.ident_str_and(|_| true)
    }
    /// Consume and return the next identifier when it matches a predicate.
    pub fn ident_and<F: FnOnce(&Ident) -> bool>(
        &mut self,
        f: F,
    ) -> Option<crate::proc_macro::Ident> {
        match self.peek_ident() {
            Some(ident) if f(ident) => match &self.bump()?.tt {
                TokenTree::Ident(ident) => Some(ident.clone()),
                _ => None,
            },
            _ => None,
        }
    }
    /// Consume and return the next identifier when its text matches a
    /// predicate (no allocation, unlike stringifying the [`Ident`]).
    pub fn ident_str_and<F: FnOnce(&str) -> bool>(
        &mut self,
        f: F,
    ) -> Option<crate::proc_macro::Ident> {
        match self.peek_ident_str() {
            Some(text) if f(text) => match &self.bump()?.tt {
                TokenTree::Ident(ident) => Some(ident.clone()),
                _ => None,
            },
            _ => None,
        }
    }
    /// Consume and return the next identifier if it is the keyword with the
    /// given index (see [`peek_keyword`](Self::peek_keyword)).
    pub fn keyword(&mut self, keyword: u8) -> Option<crate::proc_macro::Ident> {
        if self.peek_keyword()? == keyword {
            match &self.bump()?.tt {
                TokenTree::Ident(ident) => Some(ident.clone()),
                _ => None,
            }
        } else {
            None
        }
    }
    /// Consume and return the next literal token.
    pub fn literal(&mut self) -> Option<crate::proc_macro::Literal> {
        match self.entry()?.tt {
            TokenTree::Literal(_) => match &self.bump()?.tt {
                TokenTree::Literal(literal) => Some(literal.clone()),
                _ => None,
            },
            _ => None,
        }
    }
    /// Consume and return the next literal token along with its text.
    pub fn literal_with_str(&mut self) -> Option<(Literal, &str)> {
        let Kind::Literal { start, end } = self.entry()?.kind else {
            return None;
        };
        let literal = self.literal()?;
        Some((literal, &self.buffer.text[start as usize..end as usize]))
    }
    /// Consume and return the next punctuation token.
    pub fn punct(&mut self) -> Option<crate::proc_macro::Punct> {
        self.punct_char_and(|_, _| true)
    }
    /// Consume and return the next punctuation token when it matches a predicate.
    pub fn punct_and<F: FnOnce(&Punct) -> bool>(
        &mut self,
        f: F,
    ) -> Option<crate::proc_macro::Punct> {
        match self.peek_punct() {
            Some(punct) if f(punct) => match &self.bump()?.tt {
                TokenTree::Punct(punct) => Some(punct.clone()),
                _ => None,
            },
            _ => None,
        }
    }
    /// Consume and return the next punctuation token when its character and
    /// spacing (`true` for joint) match a predicate.
    pub fn punct_char_and<F: FnOnce(char, bool) -> bool>(
        &mut self,
        f: F,
    ) -> Option<crate::proc_macro::Punct> {
        match self.peek_punct_char() {
            Some((ch, joint)) if f(ch, joint) => match &self.bump()?.tt {
                TokenTree::Punct(punct) => Some(punct.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Run `f` on this cursor, rewinding it to where it was if `f` fails, so
    /// that the input is consumed only on success.
    ///
    /// `f` may move the cursor freely, including by assigning a
    /// [`clone`](Clone::clone) of it back to it, but must not replace it with
    /// a cursor over other tokens.
    ///
    /// # Errors
    /// If the argument function `f` returns an error, this error is returned.
    #[inline]
    pub fn try_advance<T, F: FnOnce(&mut Self) -> Result<T>>(&mut self, f: F) -> Result<T> {
        let checkpoint = self.checkpoint();
        let result = f(self);
        if result.is_err() {
            self.rewind(checkpoint);
        }
        result
    }

    #[inline]
    const fn checkpoint(&self) -> (u32, Span) {
        (self.pos, self.last_span)
    }

    #[inline]
    const fn rewind(&mut self, (pos, last_span): (u32, Span)) {
        self.pos = pos;
        self.last_span = last_span;
    }

    /// Try to parse a value without consuming input on failure.
    ///
    /// # Errors
    /// Return an error if parsing fails.
    #[inline]
    pub fn try_parse<T: Parse>(&mut self) -> Result<T> {
        self.try_advance(T::parse)
    }

    /// Parse a value from the current cursor.
    ///
    /// # Errors
    /// Return an error if parsing fails.
    #[inline]
    pub fn parse<T: Parse>(&mut self) -> Result<T> {
        T::parse(self)
    }

    /// Parse a value without advancing the input on failure.
    ///
    /// # Errors
    /// Return an error if parsing fails.
    #[inline]
    pub fn peek_parse<T: Peek>(&mut self) -> Result<T> {
        T::parse(self)
    }
}

impl Iterator for ParseBuffer {
    type Item = TokenTree;

    fn next(&mut self) -> Option<Self::Item> {
        self.bump().map(|entry| entry.tt.clone())
    }
}


/// Parse a whole token stream as `T`, failing if any tokens are left over.
///
/// This is what the `#[parsyng::proc_macro]` family of helper attributes use
/// to parse their input.
///
/// # Errors
/// Returns an error if parsing fails, or if `T` does not consume every
/// token.
pub fn parse_all<T: Parse>(tokens: TokenStream) -> Result<T> {
    let mut input = ParseBuffer::new(tokens);
    let value = input.parse()?;
    if input.is_empty() {
        Ok(value)
    } else {
        Err(Diagnostics::new_error_spanned(
            "Unexpected token",
            input.span(),
        ))
    }
}

impl ToTokens for ParseBuffer {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        tokens.extend(self.clone());
    }
}

/// A type that can be parsed from a [`ParseBuffer`].
///
/// This is the central trait of `parsyng`: every [`ast`](crate::ast) node
/// implements it, and so does every type it is built out of (tuples,
/// [`Option`], [`Vec`], the [`combinator`](crate::combinator) types, ...).
/// The `#[derive(Parse)]` macro (exported from `parsyng-proc-macros`, and
/// re-exported at the top of the `parsyng` facade crate) implements it
/// automatically for structs whose fields are all themselves [`Parse`].
///
/// A `parse` implementation is expected to consume, on success, exactly the
/// tokens it represents and no more (leaving the rest of the buffer for the
/// caller), and on failure to return an error without any guarantee about
/// how much of the buffer was consumed — callers that need to try several
/// alternatives should go through [`ParseBuffer::try_parse`] or
/// [`ParseBuffer::try_advance`], which roll back on failure automatically.
///
/// # Example
///
/// ```
/// use parsyng_core::error::{Diagnostics, Result};
/// use parsyng_core::parse::{Parse, ParseBuffer};
/// use parsyng_core::proc_macro::Ident;
///
/// /// Parses a bare identifier that must read "self".
/// struct SelfIdent(Ident);
///
/// impl Parse for SelfIdent {
///     fn parse(input: &mut ParseBuffer) -> Result<Self> {
///         let ident: Ident = input.parse()?;
///         if ident.to_string() == "self" {
///             Ok(Self(ident))
///         } else {
///             Err(Diagnostics::new_error_spanned("expected `self`", ident.span()))
///         }
///     }
/// }
/// ```
pub trait Parse {
    /// Parse `Self` from the front of `input`, consuming the tokens it read.
    ///
    /// # Errors
    /// Return an error if parsing fails.
    fn parse(input: &mut ParseBuffer) -> Result<Self>
    where
        Self: Sized;
}

/// Marker trait for a [`Parse`] type whose parse is safe to attempt purely to
/// test "does the next token look like this", because failure never consumes
/// input.
///
/// [`Parse`] alone gives no such guarantee — an implementation might consume
/// several tokens before discovering it doesn't match and returning an
/// error. Code that wants a real lookahead check (for example,
/// `Option<T>: Parse where T: Peek` treats a failed parse as "absent" rather
/// than propagating the error) should require `T: Peek`, not just `T:
/// Parse`. All of the token types generated by the [`Token!`](crate::Token)
/// macro implement `Peek`; wrap an arbitrary [`Parse`] type in [`Peekable`]
/// to get a (cursor-cloning, hence always-safe) `Peek` impl for it too.
pub trait Peek: Parse {
    /// A cheap check of whether `Self` may parse at the cursor: `false`
    /// guarantees that parsing fails, so callers can skip the attempt (and
    /// the error it builds). The default always says it may.
    #[inline]
    #[must_use]
    fn peek(input: &ParseBuffer) -> bool {
        let _ = input;
        true
    }
}

/// Adapts any [`Parse`] type into a [`Peek`] type by cloning the
/// [`ParseBuffer`] before attempting the parse, so a failure never advances
/// the original cursor.
///
/// Use this when you need [`Peek`]-like behavior (e.g. inside `Option<T>` or
/// [`combinator::Either`](crate::combinator::Either)) for a type that only
/// implements [`Parse`], at the cost of an extra clone per attempt.
pub struct Peekable<T> {
    inner: T,
}

impl<T> Peekable<T> {
    /// Consume the wrapper and return the parsed value.
    pub fn inner(self) -> T {
        self.inner
    }
}

impl<T: Parse> Parse for Peekable<T> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            inner: input.try_parse()?,
        })
    }
}

impl<T: Parse> Peek for Peekable<T> {}

/// A placeholder type that parses and prints nothing.
///
/// Used as the default filler for the unused type parameters of
/// [`combinator::Cons`](crate::combinator::Cons) and similar generic
/// combinators.
pub type Nothing = ();

impl Parse for Nothing {
    #[inline]
    fn parse(_input: &mut ParseBuffer) -> Result<Self> {
        Ok(())
    }
}

impl ToTokens for Nothing {
    #[inline]
    fn to_tokens(&self, _tokens: &mut TokenStream) {}
}

impl<T: Parse> Parse for Box<T> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self::new(input.parse()?))
    }
}

/// A sentinel type that always fails to parse and panics if converted back
/// to tokens.
///
/// Used as the default filler for the unused type parameters of
/// [`combinator::Either`](crate::combinator::Either), so that an `Either`
/// declared with fewer than five alternatives still type-checks without
/// ever being able to actually produce the unused variants.
#[derive(Clone, Default)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct Invalid;

impl Parse for Invalid {
    #[inline]
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Err(Diagnostics::new_error_spanned(
            "Invalid cannot be parsed",
            input.span(),
        ))
    }
}

impl ToTokens for Invalid {
    #[inline]
    fn to_tokens(&self, _tokens: &mut TokenStream) {
        unimplemented!("`Invalid` can not be converted to tokens")
    }
}
