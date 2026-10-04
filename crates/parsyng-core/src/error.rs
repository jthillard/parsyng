//! Error reporting for [`Parse`](crate::parse::Parse) implementations.
//!
//! A `parsyng` parser doesn't return `Err(String)`: it returns
//! <code>Err([Diagnostics])</code>, a spanned error
//! that, once converted with [`ToTokens`], expands to one
//! `compile_error!{ ... }` invocation per collected message, each pointing
//! at the span responsible for it. This is what lets a macro built with
//! `#[parsyng::proc_macro]` and friends surface a parse failure as a normal
//! Rust compiler error at the right location, instead of panicking the
//! proc-macro process.

use crate::ToTokens;
use crate::proc_macro::{
    Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree,
};

/// A single error message attached to a [`Span`].
///
/// Converts, via [`ToTokens`], to a `compile_error!{ "..." }` invocation
/// spanned at that location.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    content: Message,
    span: Span,
}

/// An error message, kept unformatted until it is displayed: parsers create
/// (and drop) errors constantly while trying alternatives, so building one
/// must be cheap.
#[derive(Debug, Clone)]
enum Message {
    Static(&'static str),
    Owned(String),
    /// "Expected token `...`" for up to three punctuation characters.
    #[cfg_attr(not(feature = "parsing"), allow(dead_code))]
    ExpectedToken([char; 3], u8),
}

impl core::fmt::Display for Message {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Static(message) => f.write_str(message),
            Self::Owned(message) => f.write_str(message),
            Self::ExpectedToken(chars, len) => {
                f.write_str("Expected token `")?;
                for ch in &chars[..usize::from(*len)] {
                    core::fmt::Write::write_char(f, *ch)?;
                }
                f.write_str("`")
            }
        }
    }
}

/// Conversion into an error message, avoiding an allocation for `&'static str`.
pub trait IntoMessage {
    #[doc(hidden)]
    fn into_message(self) -> MessageRepr;
}

/// Opaque storage for an error message (see [`IntoMessage`]).
#[doc(hidden)]
pub struct MessageRepr(Message);

impl IntoMessage for &'static str {
    fn into_message(self) -> MessageRepr {
        MessageRepr(Message::Static(self))
    }
}
impl IntoMessage for String {
    fn into_message(self) -> MessageRepr {
        MessageRepr(Message::Owned(self))
    }
}
impl IntoMessage for &String {
    fn into_message(self) -> MessageRepr {
        MessageRepr(Message::Owned(self.clone()))
    }
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
pub struct Diagnostics {
    // The common single-error case does not allocate.
    first: Option<Diagnostic>,
    rest: Vec<Diagnostic>,
}

impl Diagnostic {
    /// Create a message attached to `span`.
    #[must_use]
    pub fn new<T: IntoMessage>(content: T, span: Span) -> Self {
        Self {
            content: content.into_message().0,
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
        Self {
            first: None,
            rest: Vec::new(),
        }
    }
    /// Wrap a single [`Diagnostic`].
    #[must_use]
    pub const fn new(diagnostic: Diagnostic) -> Self {
        Self {
            first: Some(diagnostic),
            rest: Vec::new(),
        }
    }
    /// A single-message error spanned at [`Span::call_site`].
    ///
    /// Prefer [`new_error_spanned`](Self::new_error_spanned) whenever a more
    /// precise span is available — an error pointing at the macro call site
    /// instead of the offending tokens is much less useful to whoever hits it.
    #[must_use]
    pub fn new_error<T: IntoMessage>(error: T) -> Self {
        Self::new(Diagnostic::new(error, Span::call_site()))
    }
    /// A single-message error spanned at `span`.
    #[must_use]
    pub fn new_error_spanned<T: IntoMessage>(error: T, span: Span) -> Self {
        Self::new(Diagnostic::new(error, span))
    }
    /// "Expected token `...`" for one to three punctuation characters,
    /// formatted only if displayed.
    #[cfg(feature = "parsing")]
    pub(crate) fn expected_token(chars: &[char], span: Span) -> Self {
        let mut stored = [' '; 3];
        stored[..chars.len()].copy_from_slice(chars);
        #[allow(clippy::cast_possible_truncation)]
        let len = chars.len() as u8;
        Self::new(Diagnostic {
            content: Message::ExpectedToken(stored, len),
            span,
        })
    }
    /// Add one more message to this error.
    pub fn append(&mut self, diagnostic: Diagnostic) {
        if self.first.is_none() {
            self.first = Some(diagnostic);
        } else {
            self.rest.push(diagnostic);
        }
    }
    /// Merge another `Diagnostics`' messages into this one, preserving both.
    pub fn join(&mut self, other: Self) {
        for diagnostic in other.iter_owned() {
            self.append(diagnostic);
        }
    }
    fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.first.iter().chain(&self.rest)
    }
    fn iter_owned(self) -> impl Iterator<Item = Diagnostic> {
        self.first.into_iter().chain(self.rest)
    }
}

/// The result type returned by [`Parse::parse`](crate::parse::Parse::parse):
/// <code>Ok(T)</code>, or an <code>Err</code> holding [`Diagnostics`]
/// describing why parsing failed.
pub type Result<T> = core::result::Result<T, Diagnostics>;

impl ToTokens for Diagnostic {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        // `compile_error! { "<message>" }`, spanned at the error.
        let mut message = Literal::string(&self.content.to_string());
        message.set_span(self.span);
        let mut bang = Punct::new('!', Spacing::Alone);
        bang.set_span(self.span);
        let mut body = Group::new(Delimiter::Brace, TokenTree::from(message).into());
        body.set_span(self.span);
        tokens.extend([
            TokenTree::from(Ident::new("compile_error", self.span)),
            bang.into(),
            body.into(),
        ]);
    }
}
impl ToTokens for Diagnostics {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.iter().for_each(|diagnostic| {
            diagnostic.to_tokens(tokens);
        });
    }
}

#[cfg(all(test, feature = "fallback"))]
mod tests {
    use super::{Diagnostic, Diagnostics};
    use crate::ToTokens;
    use crate::proc_macro::Span;

    fn messages(diagnostics: &Diagnostics) -> Vec<String> {
        diagnostics.iter().map(|d| d.content.to_string()).collect()
    }

    #[test]
    fn messages_keep_their_order() {
        let mut diagnostics = Diagnostics::empty();
        assert_eq!(messages(&diagnostics), Vec::<String>::new());
        assert!(diagnostics.to_token_stream().is_empty());

        diagnostics.append(Diagnostic::new("first", Span::call_site()));
        diagnostics.append(Diagnostic::new(String::from("second"), Span::call_site()));
        let mut other = Diagnostics::new_error("third");
        other.append(Diagnostic::new(String::from("fourth"), Span::call_site()));
        diagnostics.join(other);
        diagnostics.join(Diagnostics::empty());
        assert_eq!(
            messages(&diagnostics),
            ["first", "second", "third", "fourth"]
        );

        // Joining into an empty error keeps everything too.
        let mut empty = Diagnostics::empty();
        empty.join(diagnostics);
        assert_eq!(messages(&empty), ["first", "second", "third", "fourth"]);
    }

    #[test]
    fn diagnostics_print_as_compile_errors() {
        let mut diagnostics = Diagnostics::new_error_spanned("static", Span::call_site());
        diagnostics.append(Diagnostic::new(format!("owned {}", 1), Span::call_site()));
        assert_eq!(
            diagnostics.to_token_stream().to_string(),
            r#"compile_error ! { "static" } compile_error ! { "owned 1" }"#
        );
    }

    #[cfg(feature = "parsing")]
    #[test]
    fn expected_token_messages() {
        let one = Diagnostics::expected_token(&[';'], Span::call_site());
        assert_eq!(messages(&one), ["Expected token `;`"]);
        let three = Diagnostics::expected_token(&['.', '.', '='], Span::call_site());
        assert_eq!(messages(&three), ["Expected token `..=`"]);
    }

    #[test]
    fn messages_escape_quotes() {
        let diagnostics = Diagnostics::new_error("say \"hi\"");
        assert_eq!(
            diagnostics.to_token_stream().to_string(),
            r#"compile_error ! { "say \"hi\"" }"#
        );
    }
}
