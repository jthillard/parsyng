use parsyng_core as parsyng;

use parsyng_core::quote;
use parsyng_core::{error, parse, proc_macro::TokenStream};

use crate::helper_common::{MacroFn, parse_debug};

pub fn proc_macro(args: TokenStream, input: TokenStream) -> error::Result<TokenStream> {
    let function = MacroFn::parse(input, 1, "`#[parsyng::proc_macro]` function")?;
    let dbg = function.debug_call(parse_debug(&mut parse::ParseBuffer::new(args))?);

    let attributes = &function.attributes;
    let macro_ident = function.signature.ident();
    let inner_ident = &function.inner_ident;
    let in_type = &function.param_types[0];
    let out_type = &function.out_type;

    Ok(quote! {
        #attributes
        #[proc_macro]
        pub fn #macro_ident(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
            let result = match parsyng::parse::parse_all::<#in_type>(input.into()) {
                Ok(ok) => #inner_ident(ok),
                Err(err) => return <parsyng::error::Diagnostics as parsyng::ToTokens>::to_token_stream(&err).into()
            };
            let output = <#out_type as parsyng::ToTokens>::to_token_stream(&result);
            #dbg
            output.into()
        }

        #{ function.inner_function() }
    })
}
