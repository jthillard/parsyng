use moxy::ast::ItemStruct;
use moxy::template;
use moxy::token::{ToTokens as _, TokenStream};

type Input = proc_macro::TokenStream;

fn parse(input: Input) -> Result<ItemStruct, Input> {
    let tokens: TokenStream = input.into();
    moxy::parse!(tokens as ItemStruct).map_err(|e| {
        let msg = e.to_string();
        let mut out = Input::new();
        template! { compile_error!({{ msg }}); }.to_tokens(&mut out);
        out
    })
}

fn output(tokens: &TokenStream) -> Input {
    let mut out = Input::new();
    tokens.to_tokens(&mut out);
    out
}

#[cfg(feature = "small")]
pub fn named(input: Input) -> Input {
    let input = match parse(input) {
        Ok(input) => input,
        Err(e) => return e,
    };
    let (impl_generics, ty_generics, where_clause) = input.generics.split();
    let name = &input.ident;
    let name_str = moxy::token::Lit::string(&name.to_string());
    output(&template! {
        impl {{ impl_generics }} {{ name }} {{ ty_generics }} {{ where_clause }} {
            pub const NAME: &str = {{ name_str }};
        }
    })
}

#[cfg(feature = "big")]
pub fn heap_size(input: Input) -> Input {
    use moxy::ast::{Fields, GenericParam, TypeBound};
    use moxy::token::Lit;

    let mut input = match parse(input) {
        Ok(input) => input,
        Err(e) => return e,
    };
    let bound: TypeBound = moxy::parse!("crate::HeapSize").unwrap();
    for param in input.generics.params.iter_mut() {
        if let GenericParam::Type(type_param) = param {
            if type_param.colon_punct.is_none() {
                type_param.colon_punct = Some(Default::default());
            }
            type_param.bounds.push(bound.clone());
        }
    }
    let (impl_generics, ty_generics, where_clause) = input.generics.split();
    let name = &input.ident;

    let sum = match &input.fields {
        Fields::Named(fields) => {
            let names = fields.fields.iter().map(|f| f.ident.as_ref().unwrap());
            template! { 0 @for name in names { + crate::HeapSize::heap_size_of_children(&self.{{ name }}) } }
        }
        Fields::Unnamed(fields) => {
            let indices = (0..fields.fields.len()).map(Lit::usize_unsuffixed);
            template! { 0 @for index in indices { + crate::HeapSize::heap_size_of_children(&self.{{ index }}) } }
        }
        Fields::Unit => template! { 0 },
    };

    output(&template! {
        impl {{ impl_generics }} crate::HeapSize for {{ name }} {{ ty_generics }} {{ where_clause }} {
            fn heap_size_of_children(&self) -> usize {
                {{ sum }}
            }
        }
    })
}

#[cfg(feature = "parse-bench")]
pub fn parse_file(input: Input) -> Input {
    let tokens: TokenStream = input.into();
    match moxy::parse!(tokens as moxy::ast::File) {
        Ok(file) => {
            core::hint::black_box(file);
            Input::new()
        }
        Err(e) => {
            let msg = e.to_string();
            output(&template! { compile_error!({{ msg }}); })
        }
    }
}
