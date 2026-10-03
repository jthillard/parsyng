//! Parsing shared by the `#[proc_macro]`, `#[proc_macro_attribute]` and
//! `#[proc_macro_derive]` helper attributes.

use parsyng_core as parsyng;

use parsyng_core::{
    ToTokens, Token,
    ast::{
        attributes::{Attribute, parse_outer_attributes},
        signature::FnSignature,
        r#type::Type,
    },
    error::{self, Diagnostics},
    format_ident, parse,
    proc_macro::{Ident, TokenStream},
    quote,
};

use crate::dbg_macros;

/// A function annotated with one of the helper attributes.
pub struct MacroFn {
    /// The function's outer attributes (doc comments included), moved onto
    /// the generated `#[proc_macro*]` function.
    pub attributes: Vec<Attribute>,
    /// The function's signature.
    pub signature: FnSignature,
    /// The function's parameter types, in order.
    pub param_types: Vec<Type>,
    /// The function's return type.
    pub out_type: Type,
    /// The function's body.
    pub body: TokenStream,
    /// The name of the private function the user's code is moved into.
    pub inner_ident: Ident,
}

impl MacroFn {
    /// Parse a `pub fn` taking exactly `expected_params` typed parameters
    /// and returning a value.
    pub fn parse(input: TokenStream, expected_params: usize, what: &str) -> error::Result<Self> {
        let mut stream = parse::ParseBuffer::new(input);
        let attributes = parse_outer_attributes(&mut stream);
        stream.parse::<Token![pub]>()?;
        let signature = stream.parse::<FnSignature>()?;
        let span = signature.ident().span();

        if signature.args().len() != expected_params {
            let plural = if expected_params == 1 { "" } else { "s" };
            return Err(Diagnostics::new_error_spanned(
                format!(
                    "A {what} must take exactly {expected_params} parameter{plural}, found {}",
                    signature.args().len()
                ),
                span,
            ));
        }
        let param_types = signature
            .args()
            .iter()
            .map(|param| {
                param.ty().cloned().ok_or_else(|| {
                    Diagnostics::new_error_spanned(
                        format!("The parameters of a {what} must be typed (`name: Type`)"),
                        span,
                    )
                })
            })
            .collect::<error::Result<Vec<_>>>()?;
        let out_type = signature.return_type().cloned().ok_or_else(|| {
            Diagnostics::new_error_spanned(
                format!("A {what} must return a value implementing `ToTokens`"),
                span,
            )
        })?;
        let inner_ident = format_ident!("__parsyng_{}", signature.ident());

        Ok(Self {
            attributes,
            param_types,
            out_type,
            body: stream.to_token_stream(),
            inner_ident,
            signature,
        })
    }

    /// The private function holding the user's code, with its original
    /// parameter patterns and body.
    pub fn inner_function(&self) -> TokenStream {
        let Self {
            signature,
            out_type,
            body,
            inner_ident,
            ..
        } = self;
        quote! {
            fn #inner_ident(#{ signature.args() }) -> #out_type #body
        }
    }

    /// The call printing the macro's output, if the `debug` argument was
    /// passed.
    pub fn debug_call(&self, debug: bool) -> TokenStream {
        if debug {
            dbg_macros(self.signature.ident())
        } else {
            TokenStream::new()
        }
    }
}

/// Parse the optional `debug` argument at the end of a helper attribute's
/// arguments.
pub fn parse_debug(args: &mut parse::ParseBuffer) -> error::Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }
    let ident = args.parse::<Ident>()?;
    #[allow(clippy::cmp_owned)]
    if ident.to_string() != "debug" || !args.is_empty() {
        return Err(Diagnostics::new_error_spanned(
            "Expected `debug` or no arguments.",
            ident.span(),
        ));
    }
    Ok(true)
}
