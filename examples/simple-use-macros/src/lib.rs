use parsyng::{
    Parse, ToTokens,
    ast::{crate_source::Crate, item::r#struct::Struct, literal::LiteralStr, tokens::Comma},
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

/// A tuple struct: `"text", 3`.
#[derive(Parse, ToTokens)]
pub(crate) struct Repeat(LiteralStr, Comma, u8);

/// A unit struct: parses (and emits) nothing.
#[derive(Parse, ToTokens)]
pub(crate) struct Nothing;

/// A generic struct.
#[derive(Parse, ToTokens)]
pub(crate) struct Wrapper<T>(T)
where
    T: Parse + ToTokens;

/// An enum: the first variant that parses wins.
#[derive(Parse, ToTokens)]
pub(crate) enum Value {
    Number(u8),
    Text { text: LiteralStr },
    Name(Ident),
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

/// Re-emit a number, string or identifier unchanged.
#[parsyng::proc_macro]
pub fn echo(value: Wrapper<Value>) -> Wrapper<Value> {
    value
}

/// Expand to nothing at all.
#[parsyng::proc_macro]
pub fn nothing(nothing: Nothing) -> Nothing {
    nothing
}

/// `repeat!("ab", 3)` expands to `"ababab"`.
#[parsyng::proc_macro]
pub fn repeat(Repeat(text, _, count): Repeat) -> String {
    text.value().repeat(count.into())
}
