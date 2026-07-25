//! Generic building blocks for writing [`Parse`](crate::parse::Parse)/[`ToTokens`]
//! implementations without repeating yourself: sequencing
//! ([`Cons`](crate::combinator::Cons), tuples), optionality (`Option<T>`),
//! repetition ([`Punctuated`](crate::combinator::Punctuated), `Vec<T>`,
//! [`GreedyVec`](crate::combinator::GreedyVec)) and alternation
//! ([`Either`](crate::combinator::Either)).
//!
//! These are the same pieces the [`ast`](crate::ast) module is built out of,
//! and are equally usable in your own hand-written or `#[derive(Parse)]`d
//! types.

use std::slice::{Iter, IterMut};
use std::{marker::PhantomData, vec::IntoIter};

use crate::ToTokens;

use crate::error::Diagnostics;
use crate::parse::Invalid;
use crate::{
    error::Result,
    parse::{Nothing, Parse, ParseBuffer, Peek},
};

/// Parses up to five values in sequence, one after another.
///
/// This is the fixed-arity equivalent of tuples `(A, B)`..`(A, B, C, D)`
/// (which also implement [`Parse`]/[`ToTokens`] up to 4 elements); use
/// `Cons` when you want named fields instead of positional ones, or need
/// exactly 5. Unused trailing type parameters default to [`Nothing`], which
/// parses and prints nothing, so `Cons<A, B>` is a valid 2-element sequence.
#[derive(Clone, Default, Debug)]
pub struct Cons<A, B, C = Nothing, D = Nothing, E = Nothing> {
    /// The first value.
    pub first: A,
    /// The second value.
    pub second: B,
    /// The third value.
    pub third: C,
    /// The fourth value.
    pub fourth: D,
    /// The fifth value.
    pub fifth: E,
}

impl<A: Parse, B: Parse, C: Parse, D: Parse, E: Parse> Parse for Cons<A, B, C, D, E> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            first: input.parse()?,
            second: input.parse()?,
            third: input.parse()?,
            fourth: input.parse()?,
            fifth: input.parse()?,
        })
    }
}
impl<A: ToTokens, B: ToTokens, C: ToTokens, D: ToTokens, E: ToTokens> ToTokens
    for Cons<A, B, C, D, E>
{
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.first.to_tokens(tokens);
        self.second.to_tokens(tokens);
        self.third.to_tokens(tokens);
        self.fourth.to_tokens(tokens);
        self.fifth.to_tokens(tokens);
    }
}
impl<A: Parse, B: Parse> Parse for (A, B) {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok((input.parse()?, input.parse()?))
    }
}
impl<A: Parse, B: Parse, C: Parse> Parse for (A, B, C) {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok((input.parse()?, input.parse()?, input.parse()?))
    }
}

impl<A: Parse, B: Parse, C: Parse, D: Parse> Parse for (A, B, C, D) {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok((
            input.parse()?,
            input.parse()?,
            input.parse()?,
            input.parse()?,
        ))
    }
}

/// `Option<T>: Parse` never fails: if `T: Peek` doesn't match at the current
/// position, parsing yields `None` and the cursor is left untouched, rather
/// than propagating an error.
impl<T: Peek> Parse for Option<T> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(input.parse().ok())
    }
}
impl<T: Peek> Peek for Option<T> {}

/// [`Punctuated`] error strategy: alternate parsing an element then a
/// separator until the buffer is exhausted, propagating the first parse
/// failure (of either kind) as an error.
///
/// Requires `P: Parse` (not just [`Peek`]) since it doesn't need to look
/// ahead before committing to a separator.
#[derive(Clone, Default, Debug)]
pub struct Greedy;
/// [`Punctuated`] error strategy: stop, without error, as soon as the next
/// element or separator doesn't parse, treating the tokens parsed so far as
/// the complete list.
///
/// Requires `P: Peek` so that checking for "is there another separator"
/// never consumes tokens on failure.
#[derive(Clone, Default, Debug)]
pub struct StopOnError;

/// A `T`-then-`P`-then-`T`-then-`P`-...-then-`T` sequence with an optional
/// trailing separator, mirroring `syn::punctuated::Punctuated`.
///
/// This is the type behind comma-separated lists throughout [`ast`](crate::ast)
/// — struct fields, enum variants, function parameters, generic parameters,
/// and so on. The `OnError` parameter selects how parsing behaves when an
/// element or separator fails to match; see [`Greedy`] (the default) and
/// [`StopOnError`].
///
/// # Example
///
/// ```
/// use parsyng_core::ast::tokens::Comma;
/// use parsyng_core::combinator::Punctuated;
///
/// let mut list: Punctuated<u32, Comma> = Punctuated::new();
/// assert!(list.is_empty());
/// list = Punctuated::one(1);
/// assert_eq!(list.len(), 1);
/// assert!(list.trailing().is_some());
/// ```
#[derive(Clone, Default, Debug)]
pub struct Punctuated<T, P, OnError = Greedy> {
    content: Vec<(T, P)>,
    last: Option<T>,
    _phantom: PhantomData<OnError>,
}

/// By-value iterator over a [`Punctuated`]'s elements, produced by its
/// [`IntoIterator`] implementation.
#[derive(Clone, Default, Debug)]
pub struct PunctuatedIntoIter<T, P> {
    content: IntoIter<(T, P)>,
    last: Option<T>,
}

impl<T, P, OnError> IntoIterator for Punctuated<T, P, OnError> {
    type Item = T;

    type IntoIter = PunctuatedIntoIter<T, P>;

    fn into_iter(self) -> Self::IntoIter {
        PunctuatedIntoIter {
            content: self.content.into_iter(),
            last: self.last,
        }
    }
}

impl<T, P> Iterator for PunctuatedIntoIter<T, P> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.content.next() {
            Some(v) => Some(v.0),
            None => self.last.take(),
        }
    }
}

/// Borrowing iterator over a [`Punctuated`]'s elements, produced by
/// [`Punctuated::iter`].
#[derive(Clone, Default, Debug)]
pub struct PunctuatedIter<'a, T, P> {
    content: Iter<'a, (T, P)>,
    last: Option<&'a T>,
}

/// Mutably-borrowing iterator over a [`Punctuated`]'s elements, produced by
/// [`Punctuated::iter_mut`].
#[derive(Default, Debug)]
pub struct PunctuatedIterMut<'a, T, P> {
    content: IterMut<'a, (T, P)>,
    last: Option<&'a mut T>,
}

impl<'a, T, P> Iterator for PunctuatedIter<'a, T, P> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.content.next() {
            Some(v) => Some(&v.0),
            None => self.last.take(),
        }
    }
}

impl<'a, T, P> Iterator for PunctuatedIterMut<'a, T, P> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.content.next() {
            Some(v) => Some(&mut v.0),
            None => self.last.take(),
        }
    }
}

impl<T, P, OnError> Punctuated<T, P, OnError> {
    /// Returns `true` if the list has no elements at all (not even a
    /// trailing one without a separator).
    pub const fn is_empty(&self) -> bool {
        self.content.is_empty() && self.last.is_none()
    }
    /// Number of elements in the list, including a trailing element with no
    /// following separator.
    pub const fn len(&self) -> usize {
        self.content.len() + if self.last.is_some() { 1 } else { 0 }
    }
    /// Append an `(element, separator)` pair to the end of the list, after
    /// any existing trailing element.
    pub fn push(&mut self, pair: (T, P)) {
        self.content.push(pair);
    }
    /// Prepend an `(element, separator)` pair to the front of the list.
    pub fn push_back(&mut self, pair: (T, P)) {
        self.content.insert(0, pair);
    }

    /// The last element, if it has no trailing separator after it (i.e. the
    /// list does not end in a trailing comma or similar).
    pub const fn trailing(&self) -> &Option<T> {
        &self.last
    }
    /// An empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            content: Vec::new(),
            last: None,
            _phantom: PhantomData,
        }
    }
    /// A single-element list with no trailing separator.
    pub const fn one(elem: T) -> Self {
        Self {
            content: Vec::new(),
            last: Some(elem),
            _phantom: PhantomData,
        }
    }

    /// Iterate over `(element, separator)` pairs, excluding a trailing
    /// element that has no separator after it.
    pub fn iter_pairs(&self) -> Iter<'_, (T, P)> {
        self.content.iter()
    }

    /// Iterate over every element, in order, ignoring separators.
    pub fn iter(&self) -> PunctuatedIter<'_, T, P> {
        PunctuatedIter {
            content: self.content.iter(),
            last: self.last.as_ref(),
        }
    }

    /// Iterate mutably over every element, in order, ignoring separators.
    pub fn iter_mut(&mut self) -> PunctuatedIterMut<'_, T, P> {
        PunctuatedIterMut {
            content: self.content.iter_mut(),
            last: self.last.as_mut(),
        }
    }
}

impl<'a, T, P, OnError> IntoIterator for &'a Punctuated<T, P, OnError> {
    type Item = &'a T;
    type IntoIter = PunctuatedIter<'a, T, P>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, P, OnError> IntoIterator for &'a mut Punctuated<T, P, OnError> {
    type Item = &'a mut T;
    type IntoIter = PunctuatedIterMut<'a, T, P>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T: Parse, P: Peek> Parse for Punctuated<T, P, StopOnError> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut content = Vec::new();
        let mut last = None;
        while let Ok(element) = input.try_advance(ParseBuffer::parse::<T>) {
            if let Ok(punct) = input.peek_parse() {
                content.push((element, punct));
            } else {
                last = Some(element);
                break;
            }
        }

        Ok(Self {
            content,
            last,
            _phantom: PhantomData,
        })
    }
}

impl<T: Parse, P: Parse> Parse for Punctuated<T, P, Greedy> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut content = Vec::new();
        let mut last = None;

        while !input.is_empty() {
            let element = input.parse::<T>()?;
            if input.is_empty() {
                last = Some(element);
            } else {
                content.push((element, input.parse()?));
            }
        }

        Ok(Self {
            content,
            last,
            _phantom: PhantomData,
        })
    }
}

impl<T: ToTokens, P: ToTokens, OnError> ToTokens for Punctuated<T, P, OnError> {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        for pair in &self.content {
            pair.0.to_tokens(tokens);
            pair.1.to_tokens(tokens);
        }
        self.last.to_tokens(tokens);
    }
}
/// `Vec<T>: Parse` repeatedly parses `T` until an attempt fails, then stops
/// and returns everything parsed so far — it never itself returns an error,
/// even if the buffer isn't empty afterwards (leftover tokens are left for
/// the caller to reject). Because each attempt is wrapped in
/// [`ParseBuffer::try_advance`], a partially-consumed failed attempt never
/// leaks into the result.
impl<T: Parse> Parse for Vec<T> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut content = Self::new();
        while let Ok(element) = input.try_advance(ParseBuffer::parse::<T>) {
            content.push(element);
        }

        Ok(content)
    }
}
/// Like `Vec<T>: Parse`, but requires the buffer to be fully consumed:
/// parses `T` repeatedly until the buffer is empty, propagating the first
/// error encountered instead of silently stopping.
///
/// Prefer this over `Vec<T>` when leftover, unparseable tokens after the
/// list should be reported as an error rather than left for the caller to
/// notice (or not) on their own.
pub struct GreedyVec<T> {
    inner: Vec<T>,
}
impl<T> GreedyVec<T> {
    /// Consume the wrapper and return the parsed elements.
    #[must_use]
    pub fn inner(self) -> Vec<T> {
        self.inner
    }
}
impl<T: Parse> Parse for GreedyVec<T> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut content = Vec::new();
        while !input.is_empty() {
            content.push(input.parse()?);
        }

        Ok(Self { inner: content })
    }
}

/// Tries up to five alternative [`Parse`] implementations in order and keeps
/// the first that succeeds.
///
/// Mirrors `syn`'s common `if let Ok(x) = input.parse() { ... } else if
/// ...` pattern as a reusable type. Unused trailing type parameters default
/// to [`Invalid`], a type that never
/// parses successfully, so `Either<A, B>` is a valid two-way alternative.
/// If every alternative fails, the returned error combines the diagnostics
/// from all of them.
///
/// # Example
///
/// ```no_run
/// use parsyng_core::combinator::Either;
/// use parsyng_core::parse::ParseBuffer;
/// use parsyng_core::proc_macro::Ident;
///
/// let mut input = ParseBuffer::new("42".parse().unwrap());
/// let value: Either<u32, Ident> = input.parse().unwrap();
/// assert!(matches!(value, Either::First(42)));
/// ```
#[derive(Clone, Debug)]
pub enum Either<A, B, C = Invalid, D = Invalid, E = Invalid> {
    /// The first alternative matched.
    First(A),
    /// The second alternative matched.
    Second(B),
    /// The third alternative matched.
    Third(C),
    /// The fourth alternative matched.
    Fourth(D),
    /// The fifth alternative matched.
    Fifth(E),
}

impl<A: Parse, B: Parse, C: Parse, D: Parse, E: Parse> Parse for Either<A, B, C, D, E> {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut diagnostics = Diagnostics::empty();
        match input.try_parse() {
            Ok(first) => return Ok(Self::First(first)),
            Err(err) => diagnostics.join(err),
        }
        match input.try_parse() {
            Ok(second) => return Ok(Self::Second(second)),
            Err(err) => diagnostics.join(err),
        }
        match input.try_parse() {
            Ok(third) => return Ok(Self::Third(third)),
            Err(err) => diagnostics.join(err),
        }
        match input.try_parse() {
            Ok(fourth) => return Ok(Self::Fourth(fourth)),
            Err(err) => diagnostics.join(err),
        }
        match input.try_parse() {
            Ok(fifth) => return Ok(Self::Fifth(fifth)),
            Err(err) => diagnostics.join(err),
        }
        Err(diagnostics)
    }
}

impl<A: ToTokens, B: ToTokens, C: ToTokens, D: ToTokens, E: ToTokens> ToTokens
    for Either<A, B, C, D, E>
{
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::First(first) => first.to_tokens(tokens),
            Self::Second(second) => second.to_tokens(tokens),
            Self::Third(third) => third.to_tokens(tokens),
            Self::Fourth(fourth) => fourth.to_tokens(tokens),
            Self::Fifth(fifth) => fifth.to_tokens(tokens),
        }
    }
}
