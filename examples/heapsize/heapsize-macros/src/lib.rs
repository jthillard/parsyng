// Inspired from https://github.com/dtolnay/syn

use parsyng::{
    ast::item::{DeriveInput, GenericParam, GenericParams, r#struct::StructFields},
    parse_quote,
    proc_macro::TokenStream,
    quote, quote_spanned,
};

#[parsyng::proc_macro_derive(HeapSize)]
pub fn derive_heap_size(mut input: DeriveInput) -> parsyng::proc_macro::TokenStream {
    // Add a bound `T: HeapSize` to every type parameter T.
    input.generics_parameters_mut().map(add_trait_bounds);

    let (impl_generics, ty_generics, where_clause) = input.split_generics_for_impl();

    // Generate an expression to sum up the heap size of each field.
    let sum = heap_size_sum(&input);

    let expanded = quote! {
        // The generated impl.
        impl #impl_generics heapsize::HeapSize for #{ input.ident() } #ty_generics #where_clause {
            fn heap_size_of_children(&self) -> usize {
                #sum
            }
        }
    };

    // Hand the output tokens back to the compiler.
    expanded
}

// Add a bound `T: HeapSize` to every type parameter T.
fn add_trait_bounds(generics: &mut GenericParams) {
    for param in generics.iter_mut() {
        if let GenericParam::Type(type_param) = param {
            type_param.bounds.push(parse_quote!(heapsize::HeapSize));
        }
    }
}

// Generate an expression to sum up the heap size of each field.
fn heap_size_sum(data: &DeriveInput) -> TokenStream {
    match *data {
        DeriveInput::Struct(ref data) => {
            match data.fields {
                StructFields::Named(ref fields) => {
                    // Expands to an expression like
                    //
                    //     0 + self.x.heap_size() + self.y.heap_size() + self.z.heap_size()
                    //
                    // but using fully qualified function call syntax.
                    //
                    // We take some care to use the span of each `parsyng::Field` as
                    // the span of the corresponding `heap_size_of_children`
                    // call. This way if one of the field types does not
                    // implement `HeapSize` then the compiler's error message
                    // underlines which field it is. An example is shown in the
                    // readme of the parent directory.
                    let mut recurse = fields.inner_ref().iter().map(|f| {
                        quote_spanned! { f.span() =>
                            heapsize::HeapSize::heap_size_of_children(&self.#{ f.ident })
                        }
                    });
                    quote! {
                        0 #(+ #recurse)*
                    }
                }
                StructFields::Unnamed(ref fields) => {
                    // Expands to an expression like
                    //
                    //     0 + self.0.heap_size() + self.1.heap_size() + self.2.heap_size()
                    let mut recurse = fields.inner_ref().iter().enumerate().map(|(i, f)| {
                        quote_spanned! { f.span() =>
                            heapsize::HeapSize::heap_size_of_children(&self.#i)
                        }
                    });
                    quote! {
                        0 #(+ #recurse)*
                    }
                }
                StructFields::Unit => {
                    // Unit structs cannot own more than 0 bytes of heap memory.
                    quote!(0)
                }
            }
        }
        DeriveInput::Enum(_) => unimplemented!(),
    }
}
