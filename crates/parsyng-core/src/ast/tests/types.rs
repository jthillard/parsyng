use super::{check, fails, parse_prefix, ts};
use crate as parsyng;
use crate::{
    ast::r#type::{Type, TypePath},
    proc_macro::TokenStream,
};
use parsyng_quote_macros::quote;

fn variant(tokens: TokenStream) -> &'static str {
    match check::<Type>(tokens) {
        Type::Paren(_) => "paren",
        Type::ImplTrait(_) => "impl",
        Type::Path(_) => "path",
        Type::Tuple(_) => "tuple",
        Type::Never(_) => "never",
        Type::Pointer(_) => "pointer",
        Type::Reference(_) => "reference",
        Type::Array(_) => "array",
        Type::Slice(_) => "slice",
        Type::DynTrait(_) => "dyn",
        Type::QualifiedPath(_) => "qualified",
        Type::BareFn(_) => "fn",
        Type::MacroInvocation(_) => "macro",
    }
}

#[test]
fn every_type_variant() {
    let cases = [
        (quote! { (u8) }, "paren"),
        (quote! { impl Iterator<Item = u8> + Send + 'a }, "impl"),
        (quote! { u8 }, "path"),
        (quote! { ::std::vec::Vec<u8> }, "path"),
        (quote! { () }, "tuple"),
        (quote! { (u8,) }, "tuple"),
        (quote! { (u8, &str, ) }, "tuple"),
        (quote! { ! }, "never"),
        (quote! { *const u8 }, "pointer"),
        (quote! { *mut *const u8 }, "pointer"),
        (quote! { &u8 }, "reference"),
        (quote! { &'a mut [u8] }, "reference"),
        (quote! { &&'static str }, "reference"),
        (quote! { [u8; 4] }, "array"),
        (quote! { [[u8; N]; { N + 1 }] }, "array"),
        (quote! { [u8] }, "slice"),
        (quote! { dyn Fn(u8) -> u8 + Send + 'static }, "dyn"),
        (quote! { dyn for<'a> Trait<'a> }, "dyn"),
        (quote! { <T as Iterator>::Item }, "qualified"),
        (quote! { <T>::Assoc }, "qualified"),
        (
            quote! { <Vec<T> as IntoIterator>::IntoIter::Item },
            "qualified",
        ),
        (quote! { fn() }, "fn"),
        (quote! { fn(u8, &str) -> bool }, "fn"),
        (quote! { unsafe extern "C" fn(*const u8, ...) -> i32 }, "fn"),
        (quote! { extern fn() }, "fn"),
        (quote! { ty!() }, "macro"),
        (quote! { a::ty![u8] }, "macro"),
        (quote! { ty! { u8 } }, "macro"),
    ];
    for (tokens, expected) in cases {
        let source = tokens.to_string();
        assert_eq!(variant(tokens), expected, "{source}");
    }
}

#[test]
fn type_paths() {
    check::<TypePath>(quote! { Vec<Vec<u8>> });
    check::<TypePath>(quote! { HashMap<String, Vec<(u8, u8)>> });
    check::<TypePath>(ts("Vec<Vec<Vec<u8>>>"));
    check::<TypePath>(quote! { Box<dyn Fn() -> u8> });
    check::<TypePath>(quote! { Fn(u8, u8) -> u8 });
    check::<TypePath>(quote! { FnMut() });
    check::<TypePath>(quote! { a::b::<c>::D });
    check::<TypePath>(quote! { Iterator<Item = u8> });
    check::<TypePath>(quote! { Trait<'a, T, N, { N }, Assoc: Clone> });
    check::<TypePath>(quote! { r#type::r#struct });
    check::<TypePath>(quote! { self::A });
    check::<TypePath>(quote! { super::super::A });
    check::<TypePath>(quote! { crate::A });
    check::<TypePath>(quote! { Self });
}

#[test]
fn types_stop_at_the_right_token() {
    let (_, rest) = parse_prefix::<Type>(quote! { u8, u16 });
    assert_eq!(rest, ",u16");
    let (_, rest) = parse_prefix::<Type>(quote! { Vec<u8> > 1 });
    assert_eq!(rest, ">1");
    let (_, rest) = parse_prefix::<Type>(quote! { u8 = 5 });
    assert_eq!(rest, "=5");
    let (_, rest) = parse_prefix::<Type>(quote! { &u8 { } });
    assert_eq!(rest, "{}");
    // `+` belongs to `dyn`/`impl` bounds.
    let (ty, rest) = parse_prefix::<Type>(quote! { impl A + B; });
    assert!(matches!(ty, Type::ImplTrait(_)));
    assert_eq!(rest, ";");
}

#[test]
fn invalid_types() {
    fails::<Type>(quote! {});
    fails::<Type>(quote! { 1 });
    fails::<Type>(quote! { *u8 });
    fails::<Type>(quote! { [u8 4] });
    fails::<Type>(quote! { & });
    fails::<Type>(quote! { Vec<u8 });
    fails::<Type>(quote! { <T as>::A });
}

#[test]
fn bare_fn_params_and_binders() {
    for tokens in [
        quote! { fn(x: u8, _: u16) -> u8 },
        quote! { fn(#[attr] x: u8, u8) },
        quote! { fn(_: u8, ...) },
        quote! { fn(a::B, c: d::E) },
        quote! { for<'a> fn(&'a u8) -> &'a u8 },
        quote! { for<'a, 'b> unsafe extern "C" fn(x: &'a u8, y: &'b u8) },
    ] {
        let source = tokens.to_string();
        assert_eq!(variant(tokens), "fn", "{source}");
    }
}

#[test]
fn bound_modifiers() {
    for tokens in [
        quote! { impl async Fn() -> u8 },
        quote! { impl AsyncFn(u8) + Send },
        quote! { dyn ~const Fn() },
        quote! { impl [const] Fn() + Send },
        quote! { impl const Trait },
        quote! { impl for<'a> async Fn(&'a u8) },
        quote! { impl ?Sized + Trait },
        quote! { impl !Send },
        quote! { impl (?Sized) },
    ] {
        check::<Type>(tokens);
    }
}
