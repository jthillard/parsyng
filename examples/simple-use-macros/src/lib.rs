use parsyng::{
    Parse, ToTokens,
    ast::{crate_source::Crate, item::r#struct::Struct, tokens::Comma},
    error::Result,
    proc_macro::Ident,
    proc_macro::TokenStream,
    quote,
};

#[derive(Parse, ToTokens)]
pub(crate) struct Foo {
    bar: u8,
    comma: Comma,
    then: Ident,
}

#[parsyng::proc_macro(debug)]
pub fn simple_macro(n: Crate) -> Result<TokenStream> {
    // eprintln!("{:#?}", n.2);
    let _tokens = quote! {
        #{n}

        let a = true;
        {
            let r#b = 0.3;
            r#b
        }
    };
    // println!("{}", _tokens);
    // Ok(_tokens)
    Ok(TokenStream::new())
    // Err(Diagnostics::new_error("{sen}"))
}

#[parsyng::proc_macro]
pub fn add_one(n: u8) -> u8 {
   n + 1
}

#[parsyng::proc_macro_attribute(debug)]
pub fn simple_macro_attribute(attrs: Foo, _n: Crate) -> Result<Crate> {
    let _tokens = quote! {
        #{_n}
        {
            #_n
        }
        #_n
    };
    println!("{}", quote! {#attrs});
    Ok(_n)
}

#[parsyng::proc_macro_derive(Simple, debug)]
pub fn simple_macro_derive(n: Struct) -> Result<TokenStream> {
    let _tokens = quote! {
        #{n}
    };
    println!("{}", _tokens);
    Ok(TokenStream::new())
}
