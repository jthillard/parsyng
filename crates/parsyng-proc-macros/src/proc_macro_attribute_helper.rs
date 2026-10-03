use parsyng_core as parsyng;

use parsyng_core::quote;
use parsyng_core::{error, parse, proc_macro::TokenStream};

use crate::helper_common::{MacroFn, parse_debug};

pub fn proc_macro_attribute(args: TokenStream, input: TokenStream) -> error::Result<TokenStream> {
    let function = MacroFn::parse(input, 2, "`#[parsyng::proc_macro_attribute]` function")?;
    let dbg = function.debug_call(parse_debug(&mut parse::ParseBuffer::new(args))?);

    let attributes = &function.attributes;
    let macro_ident = function.signature.ident();
    let inner_ident = &function.inner_ident;
    let attr_type = &function.param_types[0];
    let item_type = &function.param_types[1];
    let out_type = &function.out_type;

    Ok(quote! {
        #attributes
        #[proc_macro_attribute]
        pub fn #macro_ident(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
            let result = match (
                parsyng::parse::parse_all::<#attr_type>(attr.into()),
                parsyng::parse::parse_all::<#item_type>(item.into()),
            ) {
                (Ok(attr), Ok(item)) => #inner_ident(attr, item),
                (Err(mut err1), Err(err2)) => {
                    err1.join(err2);
                    return <parsyng::error::Diagnostics as parsyng::ToTokens>::to_token_stream(&err1).into();
                }
                (Err(err), _) => return <parsyng::error::Diagnostics as parsyng::ToTokens>::to_token_stream(&err).into(),
                (_, Err(err)) => return <parsyng::error::Diagnostics as parsyng::ToTokens>::to_token_stream(&err).into(),
            };
            let output = <#out_type as parsyng::ToTokens>::to_token_stream(&result);
            #dbg
            output.into()
        }

        #{ function.inner_function() }
    })
}
