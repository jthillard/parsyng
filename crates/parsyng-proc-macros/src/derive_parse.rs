use parsyng_core as parsyng;

use parsyng_core::ast::item::DeriveInput;
use parsyng_core::proc_macro::TokenStream;
use parsyng_core::quote;
use parsyng_core::{error, parse};

use crate::derive_common::Fields;

/// Build an expression constructing `path` (`Self` or `Self::Variant`) by
/// parsing each of its fields in order.
fn construct(path: &TokenStream, fields: &Fields) -> TokenStream {
    match fields {
        Fields::Named(names) => {
            let fields: Vec<_> = names
                .iter()
                .map(|name| quote! { #name: input.parse()?, })
                .collect();
            quote! { #path { #fields } }
        }
        Fields::Unnamed(count) => {
            let fields: Vec<_> = (0..*count).map(|_| quote! { input.parse()?, }).collect();
            quote! { #path ( #fields ) }
        }
        Fields::Unit => path.clone(),
    }
}

pub fn derive_parse(input: TokenStream) -> error::Result<TokenStream> {
    let input = parse::ParseBuffer::new(input).parse::<DeriveInput>()?;

    let body = match &input {
        DeriveInput::Struct(item) => {
            let fields = Fields::of_struct(&item.fields);
            let value = construct(&quote! { Self }, &fields);
            let unused = matches!(fields, Fields::Unit).then(|| quote! { let _ = input; });
            quote! {
                #unused
                Ok(#value)
            }
        }
        DeriveInput::Enum(item) => {
            let mut attempts = vec![];
            let mut names = vec![];
            for variant in item.variants() {
                let ident = variant.ident();
                let value = construct(&quote! { Self::#ident }, &Fields::of_variant(variant.fields()));
                attempts.push(quote! {
                    if let Ok(value) = input.try_advance::<Self, _>(|input| Ok(#value)) {
                        return Ok(value);
                    }
                });
                names.push(format!("`{ident}`"));
            }
            let error = format!(
                "Expected `{}`, one of: {}",
                input.ident(),
                names.join(", ")
            );
            quote! {
                #attempts
                Err(parsyng::error::Diagnostics::new_error_spanned(#error, input.span()))
            }
        }
    };

    let ident = input.ident();
    let (impl_generics, type_generics, where_clause) = input.split_generics_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics parsyng::parse::Parse for #ident #type_generics #where_clause {
            fn parse(input: &mut parsyng::parse::ParseBuffer) -> parsyng::error::Result<Self> {
                #body
            }
        }
    })
}
