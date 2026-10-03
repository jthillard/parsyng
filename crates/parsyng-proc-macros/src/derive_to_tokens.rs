use parsyng_core as parsyng;

use parsyng_core::ast::item::DeriveInput;
use parsyng_core::proc_macro::TokenStream;
use parsyng_core::{Index, error, parse, quote};

use crate::derive_common::{Fields, binding};

/// Emit `field.to_tokens(tokens);` for each of `fields`.
fn emit(fields: impl IntoIterator<Item = TokenStream>) -> Vec<TokenStream> {
    fields
        .into_iter()
        .map(|field| quote! { parsyng::ToTokens::to_tokens(#field, tokens); })
        .collect()
}

pub fn derive_to_tokens(input: TokenStream) -> error::Result<TokenStream> {
    let input = parse::ParseBuffer::new(input).parse::<DeriveInput>()?;

    let body = match &input {
        DeriveInput::Struct(item) => match Fields::of_struct(&item.fields) {
            Fields::Named(names) => {
                let fields = emit(names.iter().map(|name| quote! { &self.#name }));
                quote! { #fields }
            }
            Fields::Unnamed(count) => {
                let fields = emit((0..count).map(|i| {
                    let index = Index::from(i);
                    quote! { &self.#index }
                }));
                quote! { #fields }
            }
            Fields::Unit => quote! { let _ = tokens; },
        },
        DeriveInput::Enum(item) => {
            let mut arms = vec![];
            for variant in item.variants() {
                let ident = variant.ident();
                let arm = match Fields::of_variant(variant.fields()) {
                    Fields::Named(names) => {
                        let patterns: Vec<_> = names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| {
                                let binding = binding(i);
                                quote! { #name: #binding, }
                            })
                            .collect();
                        let fields = emit((0..names.len()).map(|i| parsyng::ToTokens::to_token_stream(&binding(i))));
                        quote! { Self::#ident { #patterns } => { #fields } }
                    }
                    Fields::Unnamed(count) => {
                        let patterns: Vec<_> = (0..count)
                            .map(|i| {
                                let binding = binding(i);
                                quote! { #binding, }
                            })
                            .collect();
                        let fields = emit((0..count).map(|i| parsyng::ToTokens::to_token_stream(&binding(i))));
                        quote! { Self::#ident ( #patterns ) => { #fields } }
                    }
                    Fields::Unit => quote! { Self::#ident => {} },
                };
                arms.push(arm);
            }
            quote! {
                match self {
                    #arms
                }
            }
        }
    };

    let ident = input.ident();
    let (impl_generics, type_generics, where_clause) = input.split_generics_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics parsyng::ToTokens for #ident #type_generics #where_clause {
            fn to_tokens(&self, tokens: &mut parsyng::proc_macro::TokenStream) {
                #body
            }
        }
    })
}
