use proc_macro::{Delimiter, TokenStream};

use crate::helper_common::{MacroFn, parse_all, parse_debug, return_error};
use crate::tokens::{Cursor, Out, Result};

pub fn proc_macro(args: TokenStream, input: TokenStream) -> Result<TokenStream> {
    let function = MacroFn::parse(input, 1, "`#[parsyng::proc_macro]` function")?;
    let debug = parse_debug(&mut Cursor::new(args))?;

    // `match <parse> { Ok(ok) => <inner>(ok), Err(err) => return <err> }`
    let mut arms = Out::new();
    arms.src("Ok(ok) =>")
        .tree(function.inner_ident.clone())
        .src("(ok), Err(err) =>")
        .src(&return_error("err"));
    let mut parse = Out::new();
    parse
        .src("match")
        .tokens(parse_all(&function.param_types[0], "input").finish())
        .group(Delimiter::Brace, arms);

    let mut kind = Out::new();
    kind.src("proc_macro");
    Ok(function.expand(kind, "(input: proc_macro::TokenStream)", parse, debug))
}
