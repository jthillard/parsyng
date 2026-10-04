//! Parsing shared by the `#[proc_macro]`, `#[proc_macro_attribute]` and
//! `#[proc_macro_derive]` helper attributes.

use proc_macro::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};

use crate::dbg_macros;
use crate::tokens::{Cursor, Error, Out, Result, is_punct, split_commas};

/// A function annotated with one of the helper attributes.
pub struct MacroFn {
    /// The function's outer attributes (doc comments included), moved onto
    /// the generated `#[proc_macro*]` function.
    pub attributes: Vec<TokenTree>,
    /// The function's name.
    pub ident: Ident,
    /// The function's parameters, as written.
    pub params: TokenStream,
    /// The function's parameter types, in order.
    pub param_types: Vec<Vec<TokenTree>>,
    /// The function's return type.
    pub out_type: Vec<TokenTree>,
    /// The function's body.
    pub body: Group,
    /// The name of the private function the user's code is moved into.
    pub inner_ident: Ident,
}

/// The type of a `pattern: Type` parameter: everything after its first `:`
/// that is not part of a `::`.
fn param_type(param: &[TokenTree]) -> Option<Vec<TokenTree>> {
    let colon = (0..param.len()).find(|&i| {
        let TokenTree::Punct(punct) = &param[i] else {
            return false;
        };
        let after_colon = i > 0
            && matches!(&param[i - 1], TokenTree::Punct(p)
                if p.as_char() == ':' && p.spacing() == proc_macro::Spacing::Joint);
        punct.as_char() == ':' && punct.spacing() == proc_macro::Spacing::Alone && !after_colon
    })?;
    let ty = param[colon + 1..].to_vec();
    (!ty.is_empty()).then_some(ty)
}

impl MacroFn {
    /// Parse a `pub fn` taking exactly `expected_params` typed parameters
    /// and returning a value.
    pub fn parse(input: TokenStream, expected_params: usize, what: &str) -> Result<Self> {
        let mut cursor = Cursor::new(input);
        let attributes = cursor.attributes();
        if !cursor.eat_ident("pub") {
            return Err(Error::new(cursor.span(), format!("A {what} must be `pub`")));
        }
        if !cursor.eat_ident("fn") {
            return Err(Error::new(cursor.span(), "Expected `fn`"));
        }
        let ident = cursor.ident("the function's name")?;
        let span = ident.span();
        let Some(params) = cursor.group(Delimiter::Parenthesis) else {
            return Err(Error::new(
                cursor.span(),
                "Expected the function's parameters",
            ));
        };

        let params_list = split_commas(params.stream());
        if params_list.len() != expected_params {
            let plural = if expected_params == 1 { "" } else { "s" };
            return Err(Error::new(
                span,
                format!(
                    "A {what} must take exactly {expected_params} parameter{plural}, found {}",
                    params_list.len()
                ),
            ));
        }
        let param_types = params_list
            .into_iter()
            .map(|param| {
                param_type(&param).ok_or_else(|| {
                    Error::new(
                        span,
                        format!("The parameters of a {what} must be typed (`name: Type`)"),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let returns = cursor.eat_punct('-') && cursor.eat_punct('>');
        let out_type = cursor
            .until(|tree| matches!(tree, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace));
        if !returns || out_type.is_empty() {
            return Err(Error::new(
                span,
                format!("A {what} must return a value implementing `ToTokens`"),
            ));
        }
        let Some(body) = cursor.group(Delimiter::Brace) else {
            return Err(Error::new(cursor.span(), "Expected the function's body"));
        };
        if !cursor.is_empty() {
            return Err(Error::new(cursor.span(), "Unexpected token"));
        }
        let inner_ident = Ident::new(&format!("__parsyng_{ident}"), Span::call_site());

        Ok(Self {
            attributes,
            ident,
            params: params.stream(),
            param_types,
            out_type,
            body,
            inner_ident,
        })
    }

    /// The private function holding the user's code, with its original
    /// parameter patterns and body.
    pub fn inner_function(&self) -> Out {
        let mut params = Out::new();
        params.tokens(self.params.clone());
        let mut out = Out::new();
        out.src("fn")
            .tree(self.inner_ident.clone())
            .group(Delimiter::Parenthesis, params)
            .src("->")
            .tokens(self.out_type.clone())
            .tree(self.body.clone());
        out
    }

    /// The call printing the macro's output, if the `debug` argument was
    /// passed.
    pub fn debug_call(&self, debug: bool) -> Out {
        if debug {
            dbg_macros(&self.ident)
        } else {
            Out::new()
        }
    }

    /// The generated `#[proc_macro*]` function: `<attrs> #[<kind>] pub fn
    /// <name>(<params>) -> proc_macro::TokenStream { <parse> <emit> }`,
    /// followed by the inner function. `parse` evaluates the inner
    /// function's result, or returns early on a parse error.
    pub fn expand(&self, kind: Out, params: &str, parse: Out, debug: bool) -> TokenStream {
        let mut body = Out::new();
        body.src("let result = ").tokens(parse.finish()).src(";");
        body.src("let output = <")
            .tokens(self.out_type.clone())
            .src("as parsyng::ToTokens>::to_token_stream(&result);")
            .tokens(self.debug_call(debug).finish())
            .src("output.into()");
        let mut out = Out::new();
        out.tokens(self.attributes.clone())
            .src("#")
            .group(Delimiter::Bracket, kind)
            .src("pub fn")
            .tree(self.ident.clone())
            .src(params)
            .src("-> proc_macro::TokenStream")
            .group(Delimiter::Brace, body)
            .tokens(self.inner_function().finish());
        out.finish()
    }
}

/// `parsyng::parse::parse_all::<Type>(<variable>.into())`
pub fn parse_all(ty: &[TokenTree], variable: &str) -> Out {
    let mut out = Out::new();
    out.src("parsyng::parse::parse_all::<")
        .tokens(ty.to_vec())
        .src(">")
        .src(&format!("({variable}.into())"));
    out
}

/// `return <parsyng::error::Diagnostics as parsyng::ToTokens>::to_token_stream(&<error>).into()`
pub fn return_error(error: &str) -> String {
    format!(
        "return <parsyng::error::Diagnostics as parsyng::ToTokens>::to_token_stream(&{error}).into()"
    )
}

/// Parse the optional `debug` argument at the end of a helper attribute's
/// arguments.
pub fn parse_debug(args: &mut Cursor) -> Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }
    let ident = args.ident("`debug` or no arguments")?;
    if ident.to_string() != "debug" || !args.is_empty() {
        return Err(Error::new(
            ident.span(),
            "Expected `debug` or no arguments.",
        ));
    }
    Ok(true)
}

/// Whether `tree` is a `,`.
pub fn is_comma(tree: Option<&TokenTree>) -> bool {
    is_punct(tree, ',')
}
