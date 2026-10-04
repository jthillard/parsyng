use super::{check, fails};
use crate as parsyng;
use crate::ast::{
    item::{
        DeriveInput, GenericParam, ItemEnum, ItemStruct, enum_item::EnumVariantFields,
        r#struct::StructFields,
    },
    visibility::Visibility,
};
use parsyng_quote_macros::quote;

#[test]
fn struct_shapes() {
    let unit = check::<ItemStruct>(quote! { struct Unit; });
    assert!(matches!(unit.fields, StructFields::Unit));

    let empty_named = check::<ItemStruct>(quote! { struct Empty {} });
    let StructFields::Named(fields) = &empty_named.fields else {
        panic!("expected named fields")
    };
    assert_eq!(fields.iter().count(), 0);

    let empty_tuple = check::<ItemStruct>(quote! { struct Empty(); });
    assert!(matches!(empty_tuple.fields, StructFields::Unnamed(_)));

    let named = check::<ItemStruct>(quote! {
        /// Docs.
        #[repr(C)]
        pub struct Named<'a, T: 'a + ?Sized> where T: Debug {
            /// Field docs.
            pub a: &'a T,
            pub(crate) b: Vec<u8>,
            c: fn() -> u8,
        }
    });
    assert_eq!(named.attributes().len(), 2);
    assert!(matches!(named.visibility(), Visibility::Public(_)));
    let StructFields::Named(fields) = &named.fields else {
        panic!("expected named fields")
    };
    let field_names: Vec<_> = fields.iter().map(|f| f.ident().to_string()).collect();
    assert_eq!(field_names, ["a", "b", "c"]);
    let first = fields.iter().next().unwrap();
    assert_eq!(first.attributes().len(), 1);
    assert!(matches!(first.visibility(), Visibility::Public(_)));

    let tuple = check::<ItemStruct>(quote! { struct Tuple<T>(#[a] T, pub u8,) where T: Clone; });
    let StructFields::Unnamed(fields) = &tuple.fields else {
        panic!("expected unnamed fields")
    };
    assert_eq!(fields.iter().count(), 2);
    assert_eq!(fields.iter().next().unwrap().attributes().len(), 1);
}

#[test]
fn enum_shapes() {
    let empty = check::<ItemEnum>(quote! { enum Never {} });
    assert!(empty.variants().is_empty());

    let item = check::<ItemEnum>(quote! {
        #[derive(Clone)]
        pub enum E<T> {
            #[default]
            A,
            B = 1 << 2,
            C(T, #[a] u8),
            D { x: T, #[a] y: u8, },
            E = { 1 + 2 },
        }
    });
    let variants: Vec<_> = item.variants().iter().collect();
    assert_eq!(variants.len(), 5);
    assert_eq!(variants[0].attributes().len(), 1);
    assert!(variants[0].discriminant().is_none());
    assert!(variants[1].discriminant().is_some());
    assert!(variants[4].discriminant().is_some());
    let EnumVariantFields::Unnamed(fields) = variants[2].fields() else {
        panic!("expected unnamed fields")
    };
    assert_eq!(fields.iter().count(), 2);
    let EnumVariantFields::Named(fields) = variants[3].fields() else {
        panic!("expected named fields")
    };
    let names: Vec<_> = fields.iter().map(|f| f.ident().to_string()).collect();
    assert_eq!(names, ["x", "y"]);
    assert!(item.where_clause().is_none());

    check::<ItemEnum>(quote! { enum E where Self: Sized { A } });
}

#[test]
fn derive_input_mutation() {
    let mut input = check::<DeriveInput>(quote! { struct S<T, U: Copy>(T, U); });
    for param in input.generics_parameters_mut().unwrap() {
        if let GenericParam::Type(param) = param {
            param
                .bounds
                .push(check::<crate::ast::item::TypeParamBound>(quote! { Clone }));
        }
    }
    let (impl_generics, type_generics, _) = input.split_generics_for_impl();
    let generated = quote! { impl #impl_generics Trait for S #type_generics {} };
    assert_eq!(
        generated.to_string().replace(' ', ""),
        "impl<T:Clone,U:Copy+Clone,>TraitforS<T,U,>{}",
    );

    let plain = check::<DeriveInput>(quote! { struct Plain; });
    assert!(plain.generics_parameters().is_none());
    let (impl_generics, type_generics, where_clause) = plain.split_generics_for_impl();
    assert!(impl_generics.is_none() && type_generics.is_none() && where_clause.is_none());
}

#[test]
fn unions() {
    let input = check::<DeriveInput>(quote! {
        #[repr(C)]
        pub union U<T: Copy> where T: Clone { pub a: T, #[doc = "b"] b: f32, }
    });
    assert_eq!(input.ident().to_string(), "U");
    assert!(input.generics_parameters().is_some());
    let DeriveInput::Union(item) = &input else {
        panic!("expected a union")
    };
    assert_eq!(item.fields().len(), 2);
    assert!(item.where_clause().is_some());
    // `union` is a weak keyword: only a keyword before the union's name.
    fails::<DeriveInput>(quote! { union { a: u8 } });
}

#[test]
fn derive_input_rejects_other_items() {
    fails::<DeriveInput>(quote! { fn f() {} });
    fails::<DeriveInput>(quote! { struct S });
    fails::<DeriveInput>(quote! { struct { a: u8 } });
    fails::<DeriveInput>(quote! { struct S { a: u8 } ; });
    fails::<DeriveInput>(quote! { enum E });
    fails::<DeriveInput>(quote! { struct S { a } });
    fails::<DeriveInput>(quote! { struct S(u8) });
    fails::<DeriveInput>(quote! { pub });
}

#[test]
fn derive_input_nodes() {
    let tuple = check::<DeriveInput>(quote! {
        #[derive(Clone)]
        pub(crate) struct Pair<T: Clone>(pub T, pub (u8, u8)) where T: Default;
    });
    assert_eq!(tuple.ident().to_string(), "Pair");
    assert_eq!(tuple.attributes().len(), 1);
    assert!(matches!(tuple.visibility(), Visibility::Crate(_, _)));
    assert!(tuple.generics_parameters().is_some());
    let DeriveInput::Struct(item) = &tuple else {
        panic!("expected a struct")
    };
    let StructFields::Unnamed(fields) = &item.fields else {
        panic!("expected a tuple struct")
    };
    assert_eq!(fields.iter().count(), 2);
    assert!(
        fields
            .iter()
            .all(|field| matches!(field.visibility(), Visibility::Public(_)))
    );

    let mut enumeration = check::<DeriveInput>(quote! {
        enum Shape<'a, T> where T: Copy {
            Unit,
            Tuple(&'a T, u8) = 3,
            Named { #[attr] x: T, y: u8 },
        }
    });
    assert_eq!(enumeration.ident().to_string(), "Shape");
    assert!(enumeration.generics_parameters().is_some());
    assert!(enumeration.generics_parameters_mut().is_some());
    let (impl_generics, type_generics, where_clause) = enumeration.split_generics_for_impl();
    let generated = quote! { impl #impl_generics Trait for Shape #type_generics #where_clause {} };
    assert_eq!(
        generated.to_string().replace(' ', ""),
        "impl<'a,T,>TraitforShape<'a,T,>whereT:Copy{}",
    );
    let DeriveInput::Enum(item) = &enumeration else {
        panic!("expected an enum")
    };
    let variants: Vec<_> = item.variants().iter().collect();
    assert_eq!(variants.len(), 3);
    assert!(matches!(variants[0].fields(), EnumVariantFields::Unit));
    assert!(matches!(
        variants[1].fields(),
        EnumVariantFields::Unnamed(_)
    ));
    assert!(variants[1].discriminant().is_some());
    let EnumVariantFields::Named(fields) = variants[2].fields() else {
        panic!("expected named fields")
    };
    let field = fields.iter().next().unwrap();
    assert_eq!(field.ident().to_string(), "x");
    assert_eq!(field.attributes().len(), 1);
}

#[test]
fn generic_defaults() {
    let input = check::<DeriveInput>(quote! {
        struct S<'a, T: Clone + 'a = u8, const N: usize = 2, const M: u8 = { 1 + 1 }>(&'a [T; N]);
    });
    // Defaults are not allowed in an `impl<...>` header.
    let (impl_generics, type_generics, _) = input.split_generics_for_impl();
    let generated = quote! { impl #impl_generics Trait for S #type_generics {} };
    assert_eq!(
        generated.to_string().replace(' ', ""),
        "impl<'a,T:Clone+'a,constN:usize,constM:u8,>TraitforS<'a,T,N,M,>{}",
    );
    check::<crate::ast::path::GenericArgs>(quote! { <3, -1, { N + 1 }> });
}
