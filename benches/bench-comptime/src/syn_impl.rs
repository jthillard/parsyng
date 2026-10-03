use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[cfg(feature = "small")]
pub fn named(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let name_str = name.to_string();
    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub const NAME: &str = #name_str;
        }
    }
    .into()
}

#[cfg(feature = "big")]
pub fn heap_size(input: TokenStream) -> TokenStream {
    use syn::{Data, Fields, GenericParam, Index, parse_quote};

    let mut input = parse_macro_input!(input as DeriveInput);
    for param in &mut input.generics.params {
        if let GenericParam::Type(type_param) = param {
            type_param.bounds.push(parse_quote!(crate::HeapSize));
        }
    }
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let name = &input.ident;

    let Data::Struct(data) = &input.data else {
        return syn::Error::new_spanned(name, "only structs are supported")
            .to_compile_error()
            .into();
    };
    let sum = match &data.fields {
        Fields::Named(fields) => {
            let names = fields.named.iter().map(|f| &f.ident);
            quote! { 0 #(+ crate::HeapSize::heap_size_of_children(&self.#names))* }
        }
        Fields::Unnamed(fields) => {
            let indices = (0..fields.unnamed.len()).map(Index::from);
            quote! { 0 #(+ crate::HeapSize::heap_size_of_children(&self.#indices))* }
        }
        Fields::Unit => quote!(0),
    };

    quote! {
        impl #impl_generics crate::HeapSize for #name #ty_generics #where_clause {
            fn heap_size_of_children(&self) -> usize {
                #sum
            }
        }
    }
    .into()
}

#[cfg(feature = "parse-bench")]
pub fn parse_file(input: TokenStream) -> TokenStream {
    match syn::parse::<syn::File>(input) {
        Ok(file) => {
            core::hint::black_box(file);
            TokenStream::new()
        }
        Err(e) => e.to_compile_error().into(),
    }
}
