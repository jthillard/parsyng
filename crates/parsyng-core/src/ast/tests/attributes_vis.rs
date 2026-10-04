use super::{check, fails, parse_prefix, ts};
use crate as parsyng;
use crate::{
    ast::{
        attributes::{Attribute, parse_inner_attributes, parse_outer_attributes},
        visibility::Visibility,
    },
    parse::ParseBuffer,
};
use parsyng_quote_macros::quote;

#[test]
fn attribute_forms() {
    for attr in [
        quote! { #[a] },
        quote! { #[a::b(c, d = 1)] },
        quote! { #[a = "x"] },
        quote! { #[cfg_attr(test, derive(Debug))] },
        quote! { #[a[b]] },
        quote! { #[a{b}] },
    ] {
        assert!(!check::<Attribute>(attr).is_inner());
    }
    assert!(check::<Attribute>(quote! { #![allow(dead_code)] }).is_inner());

    fails::<Attribute>(ts("# a"));
    fails::<Attribute>(quote! { [a] });
    fails::<Attribute>(ts("#(a)"));
}

#[test]
fn doc_comments_are_attributes() {
    let outer = check::<Attribute>(ts("/// outer doc"));
    assert!(!outer.is_inner());
    let inner = check::<Attribute>(ts("//! inner doc"));
    assert!(inner.is_inner());
    let block = check::<Attribute>(ts("/** block doc */"));
    assert!(!block.is_inner());
}

#[test]
fn attribute_lists_split_inner_and_outer() {
    let mut input = ParseBuffer::new(ts("#![inner] //! doc\n /// doc\n #[outer] struct"));
    let inner = parse_inner_attributes(&mut input);
    assert_eq!(inner.len(), 2);
    assert!(inner.iter().all(Attribute::is_inner));
    let outer = parse_outer_attributes(&mut input);
    assert_eq!(outer.len(), 2);
    assert!(outer.iter().all(|attr| !attr.is_inner()));
    assert_eq!(super::rest(&input), "struct");

    // Outer attributes stop at an inner one, and vice versa.
    let mut input = ParseBuffer::new(quote! { #![a] });
    assert!(parse_outer_attributes(&mut input).is_empty());
    assert!(!input.is_empty());
    let mut input = ParseBuffer::new(quote! { #[a] });
    assert!(parse_inner_attributes(&mut input).is_empty());
    assert!(!input.is_empty());
}

#[test]
fn visibility_stops_before_non_restrictions() {
    // `pub` followed by a tuple-struct field type, not a restriction.
    let (vis, rest) = parse_prefix::<Visibility>(quote! { pub (crate::A) });
    assert!(matches!(vis, Visibility::Public(_)));
    assert_eq!(rest, "(crate::A)");

    let (vis, rest) = parse_prefix::<Visibility>(quote! { pub(crate) fn });
    assert!(matches!(vis, Visibility::Crate(_, _)));
    assert_eq!(rest, "fn");

    let (vis, rest) = parse_prefix::<Visibility>(quote! { fn });
    assert!(matches!(vis, Visibility::Private));
    assert_eq!(rest, "fn");

    check::<Visibility>(quote! { pub(in crate::a) });
    check::<Visibility>(quote! { pub(in ::a::b) });
}

#[test]
fn visibility_nodes() {
    let private = check::<Visibility>(quote! {});
    assert!(matches!(private, Visibility::Private));

    let public = check::<Visibility>(quote! { pub });
    assert!(matches!(public, Visibility::Public(_)));

    let vis_crate = check::<Visibility>(quote! { pub(crate) });
    assert!(matches!(vis_crate, Visibility::Crate(_, _)));

    let vis_self = check::<Visibility>(quote! { pub(self) });
    assert!(matches!(vis_self, Visibility::SelfVis(_, _)));

    let vis_in = check::<Visibility>(quote! { pub(in a::b) });
    assert!(matches!(vis_in, Visibility::PubIn(_, _)));

    let vis_super = check::<Visibility>(quote! { pub(super) });
    assert!(matches!(vis_super, Visibility::Super(_, _)));

    // A parenthesized group that is not a restriction is left in the input.
    let mut input = ParseBuffer::new(quote! { pub (u8, u8) });
    assert!(matches!(
        input.parse::<Visibility>(),
        Ok(Visibility::Public(_))
    ));
    assert!(!input.is_empty());
}
