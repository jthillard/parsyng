// `.into()` is only a no-op when parsyng is built without `proc-macro2`.
#![allow(clippy::useless_conversion)]

use parsyng::ToTokens as _;
use parsyng::ast::item::DeriveInput;
use parsyng::parse::parse_all;
use parsyng::proc_macro::TokenStream;
use parsyng::quote;

type Input = proc_macro::TokenStream;

#[cfg(feature = "small")]
pub fn named(input: Input) -> Input {
    let input = match parse_all::<DeriveInput>(input.into()) {
        Ok(input) => input,
        Err(e) => return e.to_token_stream().into(),
    };
    let (impl_generics, ty_generics, where_clause) = input.split_generics_for_impl();
    let name = input.ident();
    let name_str = parsyng::proc_macro::Literal::string(&name.to_string());
    let out: TokenStream = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub const NAME: &str = #name_str;
        }
    };
    out.into()
}

#[cfg(feature = "big")]
pub fn heap_size(input: Input) -> Input {
    use parsyng::Index;
    use parsyng::ast::item::{GenericParam, r#struct::StructFields};
    use parsyng::parse_quote;

    let mut input = match parse_all::<DeriveInput>(input.into()) {
        Ok(input) => input,
        Err(e) => return e.to_token_stream().into(),
    };
    if let Some(params) = input.generics_parameters_mut() {
        for param in params.iter_mut() {
            if let GenericParam::Type(type_param) = param {
                type_param.bounds.push(parse_quote!(crate::HeapSize));
            }
        }
    }
    let (impl_generics, ty_generics, where_clause) = input.split_generics_for_impl();

    let DeriveInput::Struct(data) = &input else {
        panic!("only structs are supported");
    };
    let sum: TokenStream = match data.fields {
        StructFields::Named(ref fields) => {
            // parsyng's `#(...)*` does not see variables nested in groups yet,
            // so build each term separately.
            let mut terms = fields.inner_ref().iter().map(|f| {
                quote! { crate::HeapSize::heap_size_of_children(&self.#{ f.ident }) }
            });
            quote! { 0 #(+ #terms)* }
        }
        StructFields::Unnamed(ref fields) => {
            let mut terms = (0..fields.inner_ref().iter().count()).map(|i| {
                let index = Index::from(i);
                quote! { crate::HeapSize::heap_size_of_children(&self.#index) }
            });
            quote! { 0 #(+ #terms)* }
        }
        StructFields::Unit => quote!(0),
    };

    let out: TokenStream = quote! {
        impl #impl_generics crate::HeapSize for #{ input.ident() } #ty_generics #where_clause {
            fn heap_size_of_children(&self) -> usize {
                #sum
            }
        }
    };
    out.into()
}
