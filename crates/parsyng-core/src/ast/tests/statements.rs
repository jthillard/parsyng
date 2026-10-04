use super::{check, fails, parse_prefix};
use crate as parsyng;
use crate::{
    ast::{
        crate_source::Crate,
        delimiter::Braced,
        statements::{LetStatement, Statement},
    },
    proc_macro::TokenStream,
};
use parsyng_quote_macros::quote;

fn kind(tokens: TokenStream) -> &'static str {
    match check::<Statement>(tokens) {
        Statement::Semicolon(_) => ";",
        Statement::Item(_) => "item",
        Statement::Let(_) => "let",
        Statement::ExpressionWithBlock(_, None) => "block-like",
        Statement::ExpressionWithBlock(_, Some(_)) => "block-like;",
        Statement::ExpressionWithoutBlock(_, None) => "expr",
        Statement::ExpressionWithoutBlock(_, Some(_)) => "expr;",
    }
}

#[test]
fn let_statements() {
    check::<LetStatement>(quote! { let x = 1; });
    check::<LetStatement>(quote! { let mut x: u8 = 1; });
    check::<LetStatement>(quote! { let (a, b): (u8, u8) = (1, 2); });
    check::<LetStatement>(quote! { let S { a, .. } = s; });
    check::<LetStatement>(quote! { let Some(x) = opt else { return; }; });
    check::<LetStatement>(quote! { let Ok(x) | Err(x) = r; });
    check::<LetStatement>(quote! { let f = |x| x + 1; });
    check::<LetStatement>(quote! { let v = if a { 1 } else { 2 }; });

    // Declarations without an initializer are valid Rust.
    check::<LetStatement>(quote! { let x; });
    check::<LetStatement>(quote! { let x: u8; });

    fails::<LetStatement>(quote! { let x = 1 });
    fails::<LetStatement>(quote! { let = 1; });
}

#[test]
fn statement_kinds() {
    let cases = [
        (quote! { ; }, ";"),
        (quote! { fn inner() {} }, "item"),
        (quote! { use std::fmt; }, "item"),
        (quote! { #[derive(Clone)] struct Local; }, "item"),
        (quote! { let x = 1; }, "let"),
        (quote! { if a { b() } }, "block-like"),
        (quote! { if a { b() }; }, "block-like;"),
        (quote! { for x in y {} }, "block-like"),
        (quote! { match x { _ => {} } }, "block-like"),
        (quote! { unsafe { f() } }, "block-like"),
        // Keywords followed by `!(...)` are not macro invocations.
        (quote! { if !(a).b { c() } }, "block-like"),
        (quote! { while !(*p).done {} }, "block-like"),
        (quote! { match !(a) { _ => {} } }, "block-like"),
        (quote! { return !(a); }, "expr;"),
        (quote! { f() }, "expr"),
        (quote! { f(); }, "expr;"),
        (quote! { x = 1; }, "expr;"),
        (quote! { return; }, "expr;"),
        // Like `rustc`, a statement-position macro call is an item.
        (quote! { println!("x"); }, "item"),
    ];
    for (tokens, expected) in cases {
        let source = tokens.to_string();
        assert_eq!(kind(tokens), expected, "{source}");
    }
}

#[test]
fn statement_sequences() {
    let block = check::<Braced<Vec<Statement>>>(quote! {{
        let x = 1;
        if x > 0 { f() }
        for i in 0..x { g(i); }
        m!();
        #[allow(unused)]
        let _y = x;
        x + 1
    }});
    assert_eq!(block.inner_ref().len(), 6);

    // A block-like expression ends its statement: `{}` then `- 1` would be
    // a separate statement in Rust, not a subtraction.
    let (statement, rest) = parse_prefix::<Statement>(quote! { if a {} - 1 });
    assert!(matches!(statement, Statement::ExpressionWithBlock(_, None)));
    assert_eq!(rest, "-1");
}

#[test]
fn statement_and_crate_nodes() {
    let s1 = check::<Statement>(quote! { ; });
    assert!(matches!(s1, Statement::Semicolon(_)));

    let s2 = check::<Statement>(quote! { struct A; });
    assert!(matches!(s2, Statement::Item(_)));

    let s3 = check::<Statement>(quote! { if foo { ; }; });
    assert!(matches!(s3, Statement::ExpressionWithBlock(_, _)));

    let s4 = check::<Statement>(quote! { foo; });
    assert!(matches!(s4, Statement::ExpressionWithoutBlock(_, Some(_))));

    let s5 = check::<Statement>(quote! { foo });
    assert!(matches!(s5, Statement::ExpressionWithoutBlock(_, None)));

    check::<Crate>(quote! {
        pub struct A;
        impl A { type Assoc; const VALUE: u8; }
    });
}
