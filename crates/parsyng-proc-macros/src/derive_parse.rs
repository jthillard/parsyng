use proc_macro::{Delimiter, Ident, Literal, TokenStream};

use crate::tokens::{Data, DeriveInput, Fields, Out, Result};

/// Build an expression constructing `path` (`Self` or `Self::Variant`) by
/// parsing each of its fields in order.
fn construct(out: &mut Out, variant: Option<&Ident>, fields: &Fields) {
    out.src("Self");
    if let Some(variant) = variant {
        out.src("::").tree(variant.clone());
    }
    match fields {
        Fields::Named(names) => {
            let mut body = Out::new();
            for name in names {
                body.tree(name.clone()).src(": input.parse()?,");
            }
            out.group(Delimiter::Brace, body);
        }
        Fields::Unnamed(count) => {
            let mut body = Out::new();
            body.src(&"input.parse()?,".repeat(*count));
            out.group(Delimiter::Parenthesis, body);
        }
        Fields::Unit => {}
    }
}

pub fn derive_parse(input: TokenStream) -> Result<TokenStream> {
    let input = DeriveInput::parse(input)?;

    let mut body = Out::new();
    match &input.data {
        Data::Struct(fields) => {
            if matches!(fields, Fields::Unit) {
                body.src("let _ = input;");
            }
            let mut value = Out::new();
            construct(&mut value, None, fields);
            body.src("Ok").group(Delimiter::Parenthesis, value);
        }
        Data::Enum(variants) => {
            let mut names = Vec::new();
            for (ident, fields) in variants {
                // `if let Ok(value) = input.try_advance::<Self, _>(|input| Ok(<value>)) {
                //     return Ok(value);
                // }`
                let mut value = Out::new();
                construct(&mut value, Some(ident), fields);
                let mut ok = Out::new();
                ok.group(Delimiter::Parenthesis, value);
                let mut args = Out::new();
                args.src("|input| Ok").tokens(ok.finish());
                body.src("if let Ok(value) = input.try_advance::<Self, _>")
                    .group(Delimiter::Parenthesis, args)
                    .src("{ return Ok(value); }");
                names.push(format!("`{ident}`"));
            }
            let error = format!("Expected `{}`, one of: {}", input.ident, names.join(", "));
            let mut args = Out::new();
            args.tree(Literal::string(&error)).src(", input.span()");
            let mut err = Out::new();
            err.src("parsyng::error::Diagnostics::new_error_spanned")
                .group(Delimiter::Parenthesis, args);
            body.src("Err").group(Delimiter::Parenthesis, err);
        }
    }

    let mut function = Out::new();
    function
        .src("fn parse(input: &mut parsyng::parse::ParseBuffer) -> parsyng::error::Result<Self>")
        .group(Delimiter::Brace, body);
    let generics = input.generics;
    let mut out = Out::new();
    out.src("#[automatically_derived] impl")
        .tokens(generics.impl_generics)
        .src("parsyng::parse::Parse for")
        .tree(input.ident)
        .tokens(generics.type_generics)
        .tokens(generics.where_clause)
        .group(Delimiter::Brace, function);
    Ok(out.finish())
}
