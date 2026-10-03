//! Core parsing and quoting primitives for `parsyng`.
//!
//! This crate provides the token-stream wrapper, parsing traits, AST types,
//! and quote helpers used by the proc-macro front-end.

// TODO: Maybe some additional restrictions can be helpfull, especially on comments.
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
    // rustdoc::missing_doc_code_examples,
)]
// The docs link to the whole grammar, part of which is behind `full`.
#![cfg_attr(not(feature = "full"), allow(rustdoc::broken_intra_doc_links))]
#![allow(
    clippy::option_if_let_else,
    clippy::same_functions_in_if_condition,
    clippy::too_many_lines
)]

// Put proc_macro in a private module to avoid being able to use `proc_macro::...` directly in this crate
// This way the `fallback` feature works out of the box.
#[cfg(not(feature = "fallback"))]
mod sealed {
    pub extern crate proc_macro;
}
#[cfg(not(feature = "fallback"))]
pub use sealed::proc_macro;

/// The token types: the compiler's `proc_macro`, or `parsyng_fallback` with
/// the `fallback` feature.
#[cfg(feature = "fallback")]
pub use parsyng_fallback as proc_macro;

#[cfg(feature = "parsing")]
pub mod ast;
#[cfg(feature = "parsing")]
pub mod combinator;
pub mod error;
#[cfg(feature = "parsing")]
pub mod parse;
/// Helpers for proc-macro specific token manipulation.
pub mod proc_macro_ext;
/// Quote-related token construction helpers and macros.
#[doc(hidden)]
pub mod quote;

#[cfg(feature = "parsing")]
pub use parse::Parse;

/// Build an [`Ident`](crate::proc_macro::Ident) using `format!`-style syntax,
/// spanned at [`Span::call_site`](crate::proc_macro::Span::call_site).
///
/// ```ignore
/// use parsyng::format_ident;
///
/// let index = 3;
/// let ident = format_ident!("field_{}", index); // `field_3`
/// ```
#[macro_export]
macro_rules! format_ident {
    ($($args:tt)*) => {
        $crate::proc_macro::Ident::new(&format!($($args)*), $crate::proc_macro::Span::call_site())
    };
}

/// A type that can be appended to a [`TokenStream`](crate::proc_macro::TokenStream).
///
/// This is the counterpart to [`Parse`]: every [`ast`] node, and
/// every combinator it is built from, implements `ToTokens` so it can be fed
/// straight into `quote!`'s `#interpolation` or converted to a stand-alone
/// token stream with [`to_token_stream`](Self::to_token_stream). The
/// `#[derive(ToTokens)]` macro (exported from `parsyng-proc-macros`, and
/// re-exported at the top of the `parsyng` facade crate) implements it
/// automatically for structs whose fields are all themselves `ToTokens`.
pub trait ToTokens {
    /// Append this value's tokens to the end of `tokens`.
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream);

    /// Convert this value into a new, stand-alone token stream.
    fn to_token_stream(&self) -> crate::proc_macro::TokenStream {
        let mut token_stream = crate::proc_macro::TokenStream::new();
        self.to_tokens(&mut token_stream);
        token_stream
    }
}

/// The index of a tuple field, turning into an *unsuffixed* integer literal.
///
/// Interpolating a plain `usize` in `quote!` produces a suffixed literal
/// (`0usize`), which is not a valid field name: use `Index` to generate
/// `self.0`-style accesses instead.
///
/// ```no_run
/// # use parsyng_core as parsyng;
/// use parsyng::Index;
/// # use parsyng_quote_macros::quote;
///
/// let mut fields = (0..3).map(Index::from);
/// let sum = quote! { 0 #(+ self.#fields)* }; // `0 + self.0 + self.1 + self.2`
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Index {
    /// The field's position.
    pub index: usize,
    /// The span of the generated literal.
    pub span: crate::proc_macro::Span,
}

impl From<usize> for Index {
    fn from(index: usize) -> Self {
        Self {
            index,
            span: crate::proc_macro::Span::call_site(),
        }
    }
}

impl ToTokens for Index {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        let mut literal = crate::proc_macro::Literal::usize_unsuffixed(self.index);
        literal.set_span(self.span);
        tokens.extend(Some(crate::proc_macro::TokenTree::Literal(literal)));
    }
}

#[doc(hidden)]
/// Print the generated output of a proc-macro when debug mode is enabled.
pub fn debug_stream(macro_name: &str, call_location: &str, input: &crate::proc_macro::TokenStream) {
    let output;
    #[cfg(feature = "debug-pretty")]
    {
        use std::{
            io::Write,
            path::PathBuf,
            process::{Command, Stdio},
        };

        fn catch_rustfmt_errors(input: &crate::proc_macro::TokenStream) -> Option<String> {
            // Wrap the input in a dummy function, otherwise statements like `let` can't be formatted
            let prefix = "fn __dummy() {\n";
            let suffix = "\n}";
            let input = format!("{prefix}{input}{suffix}");

            let cargo = PathBuf::from(std::option_env!("CARGO")?);
            let mut rustfmt = cargo.parent()?.to_owned();
            rustfmt.push("rustfmt");

            let mut command = Command::new(rustfmt);
            let command = command.stdin(Stdio::piped()).stdout(Stdio::piped());
            let mut exec = command.spawn().ok()?;
            exec.stdin.take()?.write_all(input.as_bytes()).unwrap();
            let output = exec.wait_with_output().ok().and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok()
                } else {
                    None
                }
            })?;

            let output = output
                .trim()
                .strip_prefix(prefix)?
                .strip_suffix(suffix)?
                .trim();
            let output = output.replace("\n    ", "\n");

            Some(output)
        }

        output = catch_rustfmt_errors(input).unwrap_or_else(|| input.to_string());
    }
    #[cfg(not(feature = "debug-pretty"))]
    {
        output = input;
    }
    eprintln!("[DEBUG] proc-macro `{macro_name}` called at {call_location}\n{output}");
}
