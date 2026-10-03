use parsyng_core as parsyng;

use parsyng_core::{
    ast::tokens::Comma,
    error, parse,
    proc_macro::{Ident, TokenStream},
    quote,
};

use crate::helper_common::{MacroFn, parse_debug};

pub fn proc_macro_derive(args: TokenStream, input: TokenStream) -> error::Result<TokenStream> {
    let function = MacroFn::parse(input, 1, "`#[parsyng::proc_macro_derive]` function")?;

    let mut args = parse::ParseBuffer::new(args);
    let derive_ident = args.parse::<Ident>()?;
    let debug = if args.is_empty() {
        false
    } else {
        args.parse::<Comma>()?;
        parse_debug(&mut args)?
    };
    let dbg = function.debug_call(debug);

    let attributes = &function.attributes;
    let macro_ident = function.signature.ident();
    let inner_ident = &function.inner_ident;
    let item_type = &function.param_types[0];
    let out_type = &function.out_type;

    Ok(quote! {
        #attributes
        #[proc_macro_derive(#derive_ident)]
        pub fn #macro_ident(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
            let result = match parsyng::parse::parse_all::<#item_type>(item.into()) {
                Ok(item) => #inner_ident(item),
                Err(err) => return <parsyng::error::Diagnostics as parsyng::ToTokens>::to_token_stream(&err).into(),
            };
            let output = <#out_type as parsyng::ToTokens>::to_token_stream(&result);
            #dbg
            output.into()
        }

        #{ function.inner_function() }
    })
}
