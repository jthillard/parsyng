//! [`Parse`] implementations for the raw proc-macro token types themselves.
//!
//! Also includes helpers that capture a run of tokens up to a delimiter
//! without parsing them, for grammar positions that [`ast`](crate::ast)
//! intentionally leaves unparsed, such as a `const`'s default-value
//! expression or an enum discriminant.

use crate::ToTokens;

use crate::error::{Diagnostics, Result};
use crate::parse::{Parse, ParseBuffer};
use crate::proc_macro::{Group, Ident, Literal, Punct, TokenStream, TokenTree};

impl Parse for TokenStream {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(input.collect())
    }
}

impl Parse for TokenTree {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        input
            .next()
            .ok_or_else(|| Diagnostics::new_error_spanned("Expected TokenTree", input.span()))
    }
}
impl Parse for Group {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        input
            .group()
            .ok_or_else(|| Diagnostics::new_error_spanned("Expected Group", input.span()))
    }
}
impl Parse for Ident {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        input
            .ident()
            .ok_or_else(|| Diagnostics::new_error_spanned("Expected identifier", input.span()))
    }
}
impl Parse for Literal {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        input
            .literal()
            .ok_or_else(|| Diagnostics::new_error_spanned("Expected literal", input.span()))
    }
}
impl Parse for Punct {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        input
            .punct()
            .ok_or_else(|| Diagnostics::new_error_spanned("Expected punctuation", input.span()))
    }
}

#[cfg(feature = "proc-macro2")]
impl Parse for crate::sealed::proc_macro::TokenStream {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(input.collect::<TokenStream>().into())
    }
}

/// Captures every token up to (but not including) the next top-level `;`,
/// without parsing them. Used e.g. for a `const`/`static` item's
/// default-value expression.
#[derive(Clone, Debug)]
pub struct TokenStreamUntilSemicolon {
    tokens: TokenStream,
}

impl TokenStreamUntilSemicolon {
    /// The captured tokens.
    #[must_use]
    pub const fn tokens(&self) -> &TokenStream {
        &self.tokens
    }
}

impl Parse for TokenStreamUntilSemicolon {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut tokens = TokenStream::new();
        while let Some(token) = input.peek() {
            if matches!(token, TokenTree::Punct(punct) if punct.as_char() == ';') {
                break;
            }
            tokens.extend(Some(input.next().expect("peeked token must exist")));
        }
        Ok(Self { tokens })
    }
}

impl ToTokens for TokenStreamUntilSemicolon {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens);
    }
}

/// Captures every token up to (but not including) the next top-level `,`,
/// without parsing them. Used e.g. for an enum variant's discriminant
/// expression.
#[derive(Clone, Debug)]
pub struct TokenStreamUntilComma {
    tokens: TokenStream,
}

impl TokenStreamUntilComma {
    /// The captured tokens.
    #[must_use]
    pub const fn tokens(&self) -> &TokenStream {
        &self.tokens
    }
}

impl Parse for TokenStreamUntilComma {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut tokens = TokenStream::new();
        while let Some(token) = input.peek() {
            if matches!(token, TokenTree::Punct(punct) if punct.as_char() == ',') {
                break;
            }
            tokens.extend(Some(input.next().expect("peeked token must exist")));
        }
        Ok(Self { tokens })
    }
}

impl ToTokens for TokenStreamUntilComma {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens);
    }
}

/// Captures every token up to (but not including) the next top-level `,` or
/// `>`, without parsing them.
#[derive(Clone, Debug)]
pub struct TokenStreamUntilCommaOrGt {
    tokens: TokenStream,
}

impl TokenStreamUntilCommaOrGt {
    /// The captured tokens.
    #[must_use]
    pub const fn tokens(&self) -> &TokenStream {
        &self.tokens
    }
}

impl Parse for TokenStreamUntilCommaOrGt {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let mut tokens = TokenStream::new();
        while let Some(token) = input.peek() {
            if matches!(
                token,
                TokenTree::Punct(punct)
                    if punct.as_char() == ',' || punct.as_char() == '>'
            ) {
                break;
            }
            tokens.extend(Some(input.next().expect("peeked token must exist")));
        }
        Ok(Self { tokens })
    }
}

impl ToTokens for TokenStreamUntilCommaOrGt {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens);
    }
}
