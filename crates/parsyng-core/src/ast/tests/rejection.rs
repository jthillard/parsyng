//! Inputs that must be rejected, and the guarantee that a failed parse of a
//! [`Peek`](crate::parse::Peek) type leaves its input untouched.

use super::{fails, peek_fails, ts};
use crate as parsyng;
use crate::ast::{
    item::{DeriveInput, GenericParams, Lifetime},
    path::SimplePath,
    tokens::{Comma, DotDotEq, Fn, PathSep, RArrow, StructKeyword},
    r#type::Type,
    visibility::Visibility,
};
use parsyng_quote_macros::quote;

#[test]
fn tokens_peek_without_consuming() {
    peek_fails::<Comma>(quote! { ; });
    peek_fails::<PathSep>(quote! { : x });
    peek_fails::<RArrow>(quote! { - x });
    peek_fails::<DotDotEq>(quote! { .. x });
    peek_fails::<StructKeyword>(quote! { r#struct });
    peek_fails::<Fn>(quote! { fun });
}

#[test]
fn joint_punctuation_is_required() {
    // `- >` with a space is two tokens, not an arrow.
    fails::<RArrow>(ts("- >"));
    fails::<PathSep>(ts(": :"));
    fails::<DotDotEq>(ts(".. ="));
}

#[test]
fn malformed_names() {
    fails::<DeriveInput>(quote! { struct 1; });
    fails::<DeriveInput>(quote! { enum "E" { A } });
    fails::<Lifetime>(quote! { a });
    fails::<SimplePath>(quote! { a:: });
}

#[test]
fn derive_level_rejections() {
    fails::<Type>(quote! { Vec<u8 });
    fails::<Type>(quote! { u8 u8 });
    fails::<GenericParams>(quote! { <T U> });
    fails::<GenericParams>(quote! { <'a T> });
    fails::<Visibility>(quote! { pub(foo) });
    fails::<Visibility>(quote! { pub(in) });
}

/// The grammar gaps listed in the `ast` module docs are reported as errors,
/// never panics.
#[cfg(feature = "full")]
#[test]
fn documented_gaps_are_errors() {
    use crate::ast::{expression::Expression, pattern::Pattern};

    fails::<Pattern>(quote! { box x });
    fails::<Expression>(quote! { try { a? } });
    fails::<Expression>(quote! { box 1 });
}

#[cfg(feature = "full")]
#[test]
fn full_rejections() {
    use crate::ast::{crate_source::Crate, item::Item, statements::Statement};

    fails::<Item>(quote! { struct S });
    fails::<Item>(quote! { fn f() });
    fails::<Item>(quote! { impl {} });
    fails::<Item>(quote! { trait T });
    fails::<Item>(quote! { use a });
    fails::<Item>(quote! { pub });
    fails::<Item>(quote! { #[a] });
    fails::<Statement>(quote! { let x = ; });
    fails::<Crate>(quote! { fn f() {} fn });
}
