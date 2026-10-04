use proc_macro::{Delimiter, TokenStream};

use crate::helper_common::{MacroFn, parse_all, parse_debug, return_error};
use crate::tokens::{Cursor, Out, Result};

pub fn proc_macro_attribute(args: TokenStream, input: TokenStream) -> Result<TokenStream> {
    let function = MacroFn::parse(input, 2, "`#[parsyng::proc_macro_attribute]` function")?;
    let debug = parse_debug(&mut Cursor::new(args))?;

    // `match (<parse attr>, <parse item>) { .. }`
    let mut parsed = Out::new();
    parsed
        .tokens(parse_all(&function.param_types[0], "attr").finish())
        .src(",")
        .tokens(parse_all(&function.param_types[1], "item").finish());
    let mut both_failed = Out::new();
    both_failed
        .src("err1.join(err2);")
        .src(&return_error("err1"))
        .src(";");
    let mut arms = Out::new();
    arms.src("(Ok(attr), Ok(item)) =>")
        .tree(function.inner_ident.clone())
        .src("(attr, item),")
        .src("(Err(mut err1), Err(err2)) =>")
        .group(Delimiter::Brace, both_failed)
        .src("(Err(err), _) =>")
        .src(&return_error("err"))
        .src(", (_, Err(err)) =>")
        .src(&return_error("err"))
        .src(",");
    let mut parse = Out::new();
    parse
        .src("match")
        .group(Delimiter::Parenthesis, parsed)
        .group(Delimiter::Brace, arms);

    let mut kind = Out::new();
    kind.src("proc_macro_attribute");
    Ok(function.expand(
        kind,
        "(attr: proc_macro::TokenStream, item: proc_macro::TokenStream)",
        parse,
        debug,
    ))
}
