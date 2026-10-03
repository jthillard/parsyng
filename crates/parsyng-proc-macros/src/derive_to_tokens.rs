use proc_macro::{Delimiter, Ident, Literal, Span, TokenStream};

use crate::tokens::{Data, DeriveInput, Fields, Out, Result};

/// A fresh binding name for the `index`-th field, used when destructuring
/// enum variants (so that field names can't shadow the generated code's own
/// variables).
fn binding(index: usize) -> Ident {
    Ident::new(&format!("__parsyng_field_{index}"), Span::call_site())
}

/// `parsyng::ToTokens::to_tokens(<field>, tokens);`
fn emit(out: &mut Out, field: Out) {
    let mut args = Out::new();
    args.tokens(field.finish()).src(", tokens");
    out.src("parsyng::ToTokens::to_tokens")
        .group(Delimiter::Parenthesis, args)
        .src(";");
}

pub fn derive_to_tokens(input: TokenStream) -> Result<TokenStream> {
    let input = DeriveInput::parse(input)?;

    let mut body = Out::new();
    match &input.data {
        Data::Struct(Fields::Named(names)) => {
            for name in names {
                let mut field = Out::new();
                field.src("&self.").tree(name.clone());
                emit(&mut body, field);
            }
        }
        Data::Struct(Fields::Unnamed(count)) => {
            for index in 0..*count {
                let mut field = Out::new();
                field.src("&self.").tree(Literal::usize_unsuffixed(index));
                emit(&mut body, field);
            }
        }
        Data::Struct(Fields::Unit) => {
            body.src("let _ = tokens;");
        }
        Data::Enum(variants) => {
            let mut arms = Out::new();
            for (ident, fields) in variants {
                arms.src("Self::").tree(ident.clone());
                let mut fields_out = Out::new();
                let count = match fields {
                    Fields::Named(names) => {
                        let mut patterns = Out::new();
                        for (index, name) in names.iter().enumerate() {
                            patterns.tree(name.clone()).src(":").tree(binding(index)).src(",");
                        }
                        arms.group(Delimiter::Brace, patterns);
                        names.len()
                    }
                    Fields::Unnamed(count) => {
                        let mut patterns = Out::new();
                        for index in 0..*count {
                            patterns.tree(binding(index)).src(",");
                        }
                        arms.group(Delimiter::Parenthesis, patterns);
                        *count
                    }
                    Fields::Unit => 0,
                };
                for index in 0..count {
                    let mut field = Out::new();
                    field.tree(binding(index));
                    emit(&mut fields_out, field);
                }
                arms.src("=>").group(Delimiter::Brace, fields_out);
            }
            body.src("match self").group(Delimiter::Brace, arms);
        }
    }

    let mut function = Out::new();
    function
        .src("fn to_tokens(&self, tokens: &mut parsyng::proc_macro::TokenStream)")
        .group(Delimiter::Brace, body);
    let generics = input.generics;
    let mut out = Out::new();
    out.src("#[automatically_derived] impl")
        .tokens(generics.impl_generics)
        .src("parsyng::ToTokens for")
        .tree(input.ident)
        .tokens(generics.type_generics)
        .tokens(generics.where_clause)
        .group(Delimiter::Brace, function);
    Ok(out.finish())
}
