//! Error reporting for [`Parse`](crate::parse::Parse) implementations.
//!
//! A `parsyng` parser doesn't return `Err(String)`: it returns
//! `Err(`[`Diagnostics`](crate::error::Diagnostics)`)`, a spanned error
//! that, once converted with [`ToTokens`], expands to one
//! `compile_error!{ ... }` invocation per collected message, each pointing
//! at the span responsible for it. This is what lets a macro built with
//! `#[parsyng::proc_macro]` and friends surface a parse failure as a normal
//! Rust compiler error at the right location, instead of panicking the
//! proc-macro process.

use crate as parsyng;

use crate::proc_macro::{Span, TokenStream};
use crate::{ToTokens, quote_spanned};

/// A single error message attached to a [`Span`].
///
/// Converts, via [`ToTokens`], to a `compile_error!{ "..." }` invocation
/// spanned at that location.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    content: String,
    span: Span,
}

/// A collection of [`Diagnostic`]s — the error type returned by
/// [`Parse::parse`](crate::parse::Parse::parse).
///
/// Multiple diagnostics accumulate when several parse alternatives are tried
/// and all of them fail (see [`combinator::Either`](crate::combinator::Either)
/// and [`ParseBuffer::try_parse`](crate::parse::ParseBuffer::try_parse)):
/// rather than keeping only the first or the last error, `parsyng` reports
/// every alternative's failure, via [`join`](Self::join). Converting a
/// `Diagnostics` with [`ToTokens`] emits one `compile_error!{ ... }` per
/// contained message.
#[derive(Debug, Clone)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostic {
    /// Create a message attached to `span`.
    #[must_use]
    pub fn new<T: Into<String>>(content: T, span: Span) -> Self {
        Self {
            content: content.into(),
            span,
        }
    }
}
impl Diagnostics {
    /// An error value with no messages in it. Useful as an accumulator to
    /// [`join`](Self::join) other diagnostics into while trying several
    /// parse alternatives.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }
    /// Wrap a single [`Diagnostic`].
    #[must_use]
    pub fn new(diagnostic: Diagnostic) -> Self {
        Self(vec![diagnostic])
    }
    /// A single-message error spanned at [`Span::call_site`].
    ///
    /// Prefer [`new_error_spanned`](Self::new_error_spanned) whenever a more
    /// precise span is available — an error pointing at the macro call site
    /// instead of the offending tokens is much less useful to whoever hits it.
    #[must_use]
    pub fn new_error<T: Into<String>>(error: T) -> Self {
        Self::new(Diagnostic::new(error, Span::call_site()))
    }
    /// A single-message error spanned at `span`.
    #[must_use]
    pub fn new_error_spanned<T: Into<String>>(error: T, span: Span) -> Self {
        Self::new(Diagnostic::new(error, span))
    }
    /// Add one more message to this error.
    pub fn append(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }
    /// Merge another `Diagnostics`' messages into this one, preserving both.
    pub fn join(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

/// The result type returned by [`Parse::parse`](crate::parse::Parse::parse):
/// <code>Ok(T)</code>, or an <code>Err</code> holding [`Diagnostics`]
/// describing why parsing failed.
pub type Result<T> = core::result::Result<T, Diagnostics>;

impl ToTokens for Diagnostic {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(quote_spanned! { self.span =>
            compile_error!{ #{ self.content } }
        });
    }
}
impl ToTokens for Diagnostics {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.iter().for_each(|diagnostic| {
            diagnostic.to_tokens(tokens);
        });
    }
}
