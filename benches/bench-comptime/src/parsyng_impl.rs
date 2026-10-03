// `.into()` is only a no-op when parsyng is built without `fallback`.
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
            let mut names = fields.inner_ref().iter().map(|f| &f.ident);
            quote! { 0 #(+ crate::HeapSize::heap_size_of_children(&self.#names))* }
        }
        StructFields::Unnamed(ref fields) => {
            let mut indices = (0..fields.inner_ref().iter().count()).map(Index::from);
            quote! { 0 #(+ crate::HeapSize::heap_size_of_children(&self.#indices))* }
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

#[cfg(feature = "parse-bench")]
pub fn parse_file(input: Input) -> Input {
    match parse_all::<parsyng::ast::crate_source::Crate>(input.into()) {
        Ok(file) => {
            core::hint::black_box(file);
            Input::new()
        }
        Err(e) => e.to_token_stream().into(),
    }
}
