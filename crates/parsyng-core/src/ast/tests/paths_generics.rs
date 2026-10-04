use super::{check, fails, parse_prefix};
use crate as parsyng;
use crate::{
    ToTokens,
    ast::{
        generics::{ImplGenerics, TypeGenerics},
        item::{
            GenericParam, GenericParams, Lifetime, LifetimeBounds, LifetimeParam,
            LifetimeWhereClauseItem, TraitBound, TypeBoundWhereClauseItem, TypeParam,
            TypeParamBound, TypeParamBounds, WhereClause, WhereClauseItem,
        },
        path::{GenericArg, GenericArgs, SimplePath, TypePathSegment},
        r#type::{Type, TypePath},
    },
    proc_macro::TokenStream,
};
use parsyng_quote_macros::quote;

fn compact(tokens: &impl ToTokens) -> String {
    tokens.to_token_stream().to_string().replace(' ', "")
}

#[test]
fn simple_paths() {
    check::<SimplePath>(quote! { a });
    check::<SimplePath>(quote! { a::b::c });
    check::<SimplePath>(quote! { crate::a });
    check::<SimplePath>(quote! { self::a });
    check::<SimplePath>(quote! { super::a });
    check::<SimplePath>(quote! { r#fn::r#struct });

    // A simple path has no generics, and stops before them.
    let (_, rest) = parse_prefix::<SimplePath>(quote! { a::b<T> });
    assert_eq!(rest, "<T>");
    // A trailing `::` that isn't followed by a segment is left behind.
    let (_, rest) = parse_prefix::<SimplePath>(quote! { a::b::* });
    assert_eq!(rest, "::*");
    fails::<SimplePath>(quote! { ::});
    fails::<SimplePath>(quote! { 1 });
}

#[test]
fn generic_args() {
    check::<GenericArgs>(quote! { <> });
    check::<GenericArgs>(quote! { <T,> });
    check::<GenericArgs>(quote! { <'a, 'static, T> });
    check::<GenericArgs>(quote! { <Item = u8> });
    check::<GenericArgs>(quote! { <Item: Clone + 'a> });
    check::<GenericArgs>(quote! { <Assoc<'a> = &'a u8> });
    check::<GenericArgs>(quote! { <N, 3, { N * 2 }, -1, true, 'c'> });

    assert!(matches!(
        check::<GenericArg>(quote! { Vec<u8> }),
        GenericArg::Type(_)
    ));
    fails::<GenericArgs>(quote! { <u8 });
    fails::<GenericArgs>(quote! { u8> });
}

#[test]
fn path_segments() {
    check::<TypePathSegment>(quote! { Vec });
    check::<TypePathSegment>(quote! { Vec<u8> });
    check::<TypePathSegment>(quote! { Fn(u8) -> u8 });
    check::<TypePathSegment>(quote! { Fn::(u8) });
    check::<TypePath>(quote! { ::a::b<c>::d });
}

#[test]
fn generic_params() {
    let params =
        check::<GenericParams>(quote! { <'a, 'b: 'a, T: ?Sized + Clone, const N: usize, U = u8,> });
    let kinds: Vec<_> = params
        .iter()
        .map(|param| match param {
            GenericParam::Lifetime(_) => 'l',
            GenericParam::Type(_) => 't',
            GenericParam::Const(_) => 'c',
        })
        .collect();
    assert_eq!(kinds, ['l', 'l', 't', 'c', 't']);

    check::<GenericParams>(quote! { <> });
    let attributed = check::<GenericParams>(quote! {
        <#[cfg(a)] 'a, #[may_dangle] #[doc = "T"] T: Clone, #[cfg(b)] const N: usize>
    });
    let attribute_counts: Vec<_> = attributed
        .iter()
        .map(|param| match param {
            GenericParam::Lifetime(param) => param.attributes().len(),
            GenericParam::Type(param) => param.attributes().len(),
            GenericParam::Const(param) => param.attributes().len(),
        })
        .collect();
    assert_eq!(attribute_counts, [1, 2, 1]);
    check::<TypeParam>(quote! { T: for<'a> Fn(&'a u8) -> &'a u8 });
    check::<TypeParamBounds>(quote! { Clone + });
    assert!(matches!(
        check::<TypeParamBound>(quote! { ?Sized }),
        TypeParamBound::Trait(_)
    ));
    check::<TraitBound>(quote! { ?Sized });
    check::<TraitBound>(quote! { for<'a, 'b> Trait<'a, 'b> });
    fails::<GenericParams>(quote! { <T });
}

#[test]
fn impl_and_type_generics() {
    fn split(tokens: TokenStream) -> (String, String) {
        let params = check::<GenericParams>(tokens);
        (
            compact(&ImplGenerics::from(&params)),
            compact(&TypeGenerics::from(&params)),
        )
    }
    assert_eq!(split(quote! { <> }), ("<>".into(), "<>".into()));
    // Lifetime bounds stay in the `impl<...>` header only.
    assert_eq!(
        split(quote! { <'a: 'b, 'b> }),
        ("<'a:'b,'b,>".into(), "<'a,'b,>".into())
    );
    // So do attributes.
    assert_eq!(
        split(quote! { <#[cfg(x)] 'a, #[cfg(x)] T = u8> }),
        ("<#[cfg(x)]'a,#[cfg(x)]T,>".into(), "<'a,T,>".into())
    );
    // Lifetimes go first in the `impl<...>` header, and defaults are dropped.
    assert_eq!(
        split(quote! { <T: Clone = u8, 'a, const N: usize = 3> }),
        ("<'a,T:Clone,constN:usize,>".into(), "<T,'a,N,>".into())
    );
}

#[test]
fn where_clauses() {
    check::<WhereClause>(quote! { where });
    check::<WhereClause>(quote! { where T: Clone, });
    check::<WhereClause>(quote! { where Vec<T>: Clone, <T as Trait>::Assoc: Copy, [T; 3]: Sized });
    check::<WhereClause>(quote! { where for<'a> &'a T: IntoIterator<Item = &'a u8> });
    check::<WhereClause>(quote! { where 'a: 'b + 'c, T: 'a });
    // A where clause stops before the item body.
    let (_, rest) = parse_prefix::<WhereClause>(quote! { where T: Clone { x: T } });
    assert_eq!(rest, concat!("{", "x:T}"));
    let (_, rest) = parse_prefix::<WhereClause>(quote! { where T: Clone; });
    assert_eq!(rest, ";");
}

#[test]
fn path_and_type_nodes() {
    check::<SimplePath>(quote! { ::core::fmt });
    check::<TypePathSegment>(quote! { Vec::<u8> });
    check::<GenericArgs>(quote! { <u8, 'a> });

    let type_arg = check::<GenericArg>(quote! { u8 });
    assert!(matches!(type_arg, GenericArg::Type(_)));
    let lt_arg = check::<GenericArg>(quote! { 'a });
    assert!(matches!(lt_arg, GenericArg::Lifetime(_)));

    check::<TypePath>(quote! { ::std::vec::Vec::<u8> });

    let ty = check::<Type>(quote! { std::vec::Vec::<u8> });
    assert!(matches!(ty, Type::Path(_)));
}

#[test]
fn generic_and_where_nodes() {
    check::<Lifetime>(quote! { 'a });

    check::<LifetimeBounds>(quote! { 'a + 'b });
    check::<LifetimeParam>(quote! { 'a: 'b + 'c });
    check::<TraitBound>(quote! { (?for<'a> Foo<'a>) });

    let bound_lt = check::<TypeParamBound>(quote! { 'a });
    assert!(matches!(bound_lt, TypeParamBound::Lifetime(_)));
    let bound_trait = check::<TypeParamBound>(quote! { Foo<'a> });
    assert!(matches!(bound_trait, TypeParamBound::Trait(_)));

    check::<TypeParamBounds>(quote! { Foo<'a> + 'a });
    check::<TypeParam>(quote! { T: Foo<'a> + 'a = U });

    let gp_ty = check::<GenericParam>(quote! { T });
    assert!(matches!(gp_ty, GenericParam::Type(_)));
    let gp_lt = check::<GenericParam>(quote! { 'a: 'b });
    assert!(matches!(gp_lt, GenericParam::Lifetime(_)));

    check::<GenericParams>(quote! { <T, 'a> });
    check::<LifetimeWhereClauseItem>(quote! { 'a: 'b + 'c });
    check::<TypeBoundWhereClauseItem>(quote! { for<'a> T: Foo<'a> + 'a });

    let wc_lt = check::<WhereClauseItem>(quote! { 'a: 'b });
    assert!(matches!(wc_lt, WhereClauseItem::Lifetime(_)));
    let wc_ty = check::<WhereClauseItem>(quote! { T: Foo<'a> });
    assert!(matches!(wc_ty, WhereClauseItem::Type(_)));

    check::<WhereClause>(quote! { where 'a: 'b, T: Foo<'a> });
}
