use super::{check, fails};
use crate as parsyng;
use crate::ast::signature::{FnParam, FnSignature};
use parsyng_quote_macros::quote;

fn params(signature: &FnSignature) -> Vec<&'static str> {
    signature
        .args()
        .iter()
        .map(|param| match param {
            FnParam::SelfParam(_) => "self",
            FnParam::Typed(_) => "typed",
            FnParam::Variadic(_) => "...",
        })
        .collect()
}

#[test]
fn self_receivers() {
    for receiver in [
        quote! { fn f(self) },
        quote! { fn f(mut self) },
        quote! { fn f(&self) },
        quote! { fn f(&mut self) },
        quote! { fn f(&'a self) },
        quote! { fn f(&'a mut self) },
        quote! { fn f(self: Box<Self>) },
        quote! { fn f(mut self: Pin<&mut Self>) },
        quote! { fn f(self,) },
    ] {
        let source = receiver.to_string();
        let signature = check::<FnSignature>(receiver);
        assert_eq!(params(&signature), ["self"], "{source}");
    }

    let signature = check::<FnSignature>(quote! { fn f(&mut self, x: u8) });
    let receiver = signature.args().iter().next().unwrap();
    assert!(receiver.mutability().is_some());
    assert!(receiver.ty().is_none());
    assert!(receiver.ident().is_none());

    let signature = check::<FnSignature>(quote! { fn f(self: Box<Self>) });
    assert!(signature.args().iter().next().unwrap().ty().is_some());
}

#[test]
fn typed_params() {
    let signature = check::<FnSignature>(quote! {
        fn f(a: u8, mut b: &str, (c, d): (u8, u8), S { e, .. }: S, _: u8, &f: &u8)
    });
    assert_eq!(params(&signature), ["typed"; 6]);
    let names: Vec<_> = signature
        .args()
        .iter()
        .map(|param| param.ident().map(ToString::to_string))
        .collect();
    assert_eq!(
        names,
        [
            Some("a".to_owned()),
            Some("b".to_owned()),
            None,
            None,
            Some("_".to_owned()),
            Some("f".to_owned()),
        ]
    );
    assert!(
        signature
            .args()
            .iter()
            .nth(1)
            .unwrap()
            .mutability()
            .is_some()
    );
    assert!(
        signature
            .args()
            .iter()
            .next()
            .unwrap()
            .mutability()
            .is_none()
    );

    let variadic =
        check::<FnSignature>(quote! { unsafe extern "C" fn printf(fmt: *const u8, ...) -> i32 });
    assert_eq!(params(&variadic), ["typed", "..."]);
}

#[test]
fn qualifiers_generics_and_return_types() {
    check::<FnSignature>(quote! { const fn f() });
    check::<FnSignature>(quote! { async fn f() });
    check::<FnSignature>(quote! { unsafe fn f() });
    check::<FnSignature>(quote! { extern fn f() });
    check::<FnSignature>(quote! { extern "system" fn f() });
    check::<FnSignature>(quote! { const async unsafe extern "C" fn f() });
    check::<FnSignature>(quote! { fn f<'a, T: 'a, const N: usize>(x: [&'a T; N]) where T: Clone });

    let unit = check::<FnSignature>(quote! { fn f() });
    assert!(unit.return_type().is_none());
    assert!(unit.args().is_empty());
    assert_eq!(unit.ident().to_string(), "f");

    let never = check::<FnSignature>(quote! { fn f() -> ! });
    assert!(never.return_type().is_some());
    check::<FnSignature>(quote! { fn f() -> impl Fn() -> u8 });
    check::<FnSignature>(quote! { fn f() -> Result<(), Box<dyn Error + Send + Sync>> });

    fails::<FnSignature>(quote! { fn () });
    fails::<FnSignature>(quote! { fn f });
    fails::<FnSignature>(quote! { fn f(a) });
    fails::<FnSignature>(quote! { fn f() -> });
}

#[test]
fn param_attributes_and_named_variadics() {
    let signature = check::<FnSignature>(quote! {
        fn f(#[a] self: Box<Self>, #[cfg(x)] #[b] x: u8, #[c] ...)
    });
    let counts: Vec<_> = signature
        .args()
        .iter()
        .map(|param| param.attributes().len())
        .collect();
    assert_eq!(counts, [1, 2, 1]);
    check::<FnSignature>(quote! { fn f(#[a] &mut self) });
    let variadic = check::<FnSignature>(quote! { unsafe extern "C" fn f(x: i32, args: ...) });
    assert_eq!(params(&variadic), ["typed", "..."]);
    check::<FnSignature>(quote! { unsafe extern "C" fn f(x: i32, mut args: ...) });
}
