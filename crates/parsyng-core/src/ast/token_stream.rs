//! [`Parse`] implementations for the raw proc-macro token types themselves.
//!
//! Also includes helpers that capture a run of tokens up to a delimiter
//! without parsing them, for grammar positions that [`ast`](crate::ast)
//! leaves unparsed so that they are available without the `full` feature,
//! such as an enum discriminant.

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

/// Captures every token up to (but not including) the next top-level `;`,
/// without parsing them. Used e.g. for a `const`/`static` item's
/// default-value expression.
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
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
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
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
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
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

#[cfg(all(test, feature = "fallback"))]
mod tests {
    use super::{TokenStreamUntilComma, TokenStreamUntilCommaOrGt, TokenStreamUntilSemicolon};
    use crate as parsyng;
    use crate::parse::{Parse, ParseBuffer};
    use crate::proc_macro::TokenStream;
    use parsyng_quote_macros::quote;

    /// Parse a `T` and return its tokens and the tokens left, printed.
    fn split<T: Parse + crate::ToTokens>(tokens: TokenStream) -> (String, String) {
        let mut input = ParseBuffer::new(tokens);
        let value = input.parse::<T>().unwrap();
        (
            value.to_token_stream().to_string(),
            input.collect::<TokenStream>().to_string(),
        )
    }

    #[test]
    fn until_semicolon() {
        let (taken, rest) = split::<TokenStreamUntilSemicolon>(quote! { a + { b; c } , (d;) ; e });
        assert_eq!(taken, quote! { a + { b; c } , (d;) }.to_string());
        assert_eq!(rest, "; e");
        let (taken, rest) = split::<TokenStreamUntilSemicolon>(quote! { a b });
        assert_eq!(taken, "a b");
        assert_eq!(rest, "");
        let (taken, rest) = split::<TokenStreamUntilSemicolon>(quote! { ; });
        assert_eq!(taken, "");
        assert_eq!(rest, ";");
    }

    #[test]
    fn until_comma() {
        let (taken, rest) = split::<TokenStreamUntilComma>(quote! { f(a, b) + [c, d] , e });
        assert_eq!(taken, quote! { f(a, b) + [c, d] }.to_string());
        assert_eq!(rest, ", e");
        // Not generics-aware: `,` inside `<...>` ends the capture.
        let (taken, rest) = split::<TokenStreamUntilComma>(quote! { A<B, C> });
        assert_eq!(taken, "A < B");
        assert_eq!(rest, ", C >");
        let (taken, rest) = split::<TokenStreamUntilComma>(quote! { a; b });
        assert_eq!(taken, "a ; b");
        assert_eq!(rest, "");
    }

    #[test]
    fn until_comma_or_gt() {
        let (taken, rest) = split::<TokenStreamUntilCommaOrGt>(quote! { { N > 1 } > });
        assert_eq!(taken, quote! { { N > 1 } }.to_string());
        assert_eq!(rest, ">");
        let (taken, rest) = split::<TokenStreamUntilCommaOrGt>(quote! { 3, 4 });
        assert_eq!(taken, "3");
        assert_eq!(rest, ", 4");
    }

    #[test]
    fn captures_round_trip() {
        let tokens = quote! { a::b(c) };
        let mut input = ParseBuffer::new(tokens.clone());
        let captured = input.parse::<TokenStreamUntilSemicolon>().unwrap();
        assert_eq!(captured.tokens().to_string(), tokens.to_string());
    }
}
