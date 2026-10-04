use proc_macro::{Delimiter, TokenStream};

use crate::helper_common::{MacroFn, is_comma, parse_all, parse_debug, return_error};
use crate::tokens::{Cursor, Error, Out, Result};

pub fn proc_macro_derive(args: TokenStream, input: TokenStream) -> Result<TokenStream> {
    let function = MacroFn::parse(input, 1, "`#[parsyng::proc_macro_derive]` function")?;

    let mut args = Cursor::new(args);
    let derive_ident = args.ident("the derived trait's name")?;
    let debug = if args.is_empty() {
        false
    } else {
        if !is_comma(args.next().as_ref()) {
            return Err(Error::new(args.span(), "Expected `,`"));
        }
        parse_debug(&mut args)?
    };

    let mut arms = Out::new();
    arms.src("Ok(item) =>")
        .tree(function.inner_ident.clone())
        .src("(item), Err(err) =>")
        .src(&return_error("err"));
    let mut parse = Out::new();
    parse
        .src("match")
        .tokens(parse_all(&function.param_types[0], "item").finish())
        .group(Delimiter::Brace, arms);

    let mut derived = Out::new();
    derived.tree(derive_ident);
    let mut kind = Out::new();
    kind.src("proc_macro_derive")
        .group(Delimiter::Parenthesis, derived);
    Ok(function.expand(kind, "(item: proc_macro::TokenStream)", parse, debug))
}
