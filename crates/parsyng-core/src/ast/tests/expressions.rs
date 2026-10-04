use super::{check, fails, parse_prefix, ts};
use crate as parsyng;
use crate::{
    ast::expression::{
        ArrayElements, ArrayExpression, AwaitExpression, BlockExpression, BreakExpression,
        CallExpression, ContinueExpression, ElseExpression, Expression, ExpressionWithBlock,
        ExpressionWithoutBlock, FieldExpression, GroupedExpression, IfExpression, IndexExpression,
        LoopExpression, MatchExpression, RangeExpression, ReturnExpression, TupleExpression,
        TupleIndexExpression, UnderscoreExpression, UnsafeBlockExpression,
    },
    proc_macro::TokenStream,
};
use parsyng_quote_macros::quote;

fn kind(tokens: TokenStream) -> &'static str {
    use ExpressionWithBlock as B;
    use ExpressionWithoutBlock as E;
    match check::<Expression>(tokens) {
        Expression::WithBlock(expr) => match *expr {
            B::Block(_) => "block",
            B::Unsafe(_) => "unsafe",
            B::Loop(_) => "loop",
            B::If(_) => "if",
            B::While(_) => "while",
            B::For(_) => "for",
            B::Match(_) => "match",
            B::Async(_) => "async",
            B::ConstBlock(_) => "const",
        },
        Expression::WithoutBlock(expr) => match *expr {
            E::Literal(_) => "literal",
            E::Path(_) => "path",
            E::QualifiedPath(_) => "qualified path",
            E::Await(_) => "await",
            E::Index(_) => "index",
            E::Array(_) => "array",
            E::Tuple(_) => "tuple",
            E::TupleIndex(_) => "tuple index",
            E::Field(_) => "field",
            E::Return(_) => "return",
            E::Continue(_) => "continue",
            E::Break(_) => "break",
            E::Underscore(_) => "underscore",
            E::Grouped(_) => "grouped",
            E::Call(_) => "call",
            E::Range(_) => "range",
            E::Unary(_) => "unary",
            E::Binary(_) => "binary",
            E::Cast(_) => "cast",
            E::Assignment(_) => "assignment",
            E::CompoundAssignment(_) => "compound assignment",
            E::Try(_) => "try",
            E::MethodCall(_) => "method call",
            E::Closure(_) => "closure",
            E::Struct(_) => "struct",
            E::MacroCall(_) => "macro call",
            E::Attributed(_) => "attributed",
        },
    }
}

#[test]
fn expression_kinds() {
    let cases = [
        (quote! { {} }, "block"),
        (quote! { 'a: { 1 } }, "block"),
        (quote! { unsafe { f() } }, "unsafe"),
        (quote! { 'outer: loop { break 'outer 1; } }, "loop"),
        (quote! { if a { b } else if c { d } else { e } }, "if"),
        (quote! { if let Some(x) = y && x > 0 { } }, "if"),
        (
            quote! { 'l: while i < 10 { i += 1; continue 'l; } },
            "while",
        ),
        (quote! { for (i, x) in xs.iter().enumerate() {} }, "for"),
        (
            quote! { match x { 0 => a, 1 | 2 if y => { b } _ => c, } },
            "match",
        ),
        (quote! { async move { x.await } }, "async"),
        (quote! { const { N * 2 } }, "const"),
        (quote! { 1u8 }, "literal"),
        (quote! { "s" }, "literal"),
        (quote! { true }, "path"),
        (quote! { a::b::<T>::c }, "path"),
        (quote! { <T as Default>::default }, "qualified path"),
        (quote! { fut.await }, "await"),
        (quote! { a[0][1] }, "index"),
        (quote! { [1, 2, 3,] }, "array"),
        (quote! { [0u8; 32] }, "array"),
        (quote! { () }, "tuple"),
        (quote! { (1,) }, "tuple"),
        (quote! { a.b.c }, "field"),
        (quote! { return }, "return"),
        (quote! { return Ok(()) }, "return"),
        (quote! { continue }, "continue"),
        (quote! { break }, "break"),
        (quote! { break 'a }, "break"),
        (quote! { _ }, "underscore"),
        (quote! { (a + b) }, "grouped"),
        (quote! { f(a, b,) }, "call"),
        (quote! { f(a)(b) }, "call"),
        (quote! { .. }, "range"),
        (quote! { a.. }, "range"),
        (quote! { ..=b }, "range"),
        (quote! { a..b }, "range"),
        (quote! { -x }, "unary"),
        (quote! { !x }, "unary"),
        (quote! { *x }, "unary"),
        (quote! { &mut x }, "unary"),
        (quote! { a * b + c }, "binary"),
        (quote! { x as u8 }, "cast"),
        (quote! { a = b }, "assignment"),
        (quote! { a += 1 }, "compound assignment"),
        (quote! { f()? }, "try"),
        (quote! { x.f::<T>(1) }, "method call"),
        (quote! { |x: u8, y| -> u8 { x + y } }, "closure"),
        (quote! { move || x }, "closure"),
        (quote! { S { a: 1, b, ..Default::default() } }, "struct"),
        (quote! { S::<T> { 0: a } }, "struct"),
        (quote! { vec![1, 2] }, "macro call"),
        (quote! { #[allow(x)] f() }, "attributed"),
    ];
    for (tokens, expected) in cases {
        let source = tokens.to_string();
        assert_eq!(kind(tokens), expected, "{source}");
    }
    assert_eq!(kind(ts("x.0")), "tuple index");
    assert_eq!(kind(ts("1.5e3")), "literal");
}

#[test]
fn postfix_chains() {
    check::<Expression>(quote! { a.b().c[0].d?.await.e(f).0 });
    check::<Expression>(quote! { client.get(url).send().await?.json::<T>().await? });
    check::<Expression>(quote! { (a.0).1 });
    check::<Expression>(quote! { f()()() });
    check::<Expression>(quote! { {}.await });
}

#[test]
fn expressions_stop_at_the_right_token() {
    let (_, rest) = parse_prefix::<Expression>(quote! { a + b, c });
    assert_eq!(rest, ",c");
    let (_, rest) = parse_prefix::<Expression>(quote! { a; b });
    assert_eq!(rest, ";b");
    let (_, rest) = parse_prefix::<Expression>(quote! { f(x) => y });
    assert_eq!(rest, "=>y");
}

#[test]
fn match_arms() {
    check::<MatchExpression>(quote! { match x {} });
    check::<MatchExpression>(quote! { match x { _ => {} } });
    check::<MatchExpression>(quote! { match x { a => {} b => {}, c => d } });
    check::<MatchExpression>(quote! { match x { #[cfg(a)] A => 1, B(y) if y > 1 => 2, _ => 3, } });
    check::<MatchExpression>(
        quote! { match (a, b) { (Some(x), _) | (_, Some(x)) => x, _ => return, } },
    );
    fails::<MatchExpression>(quote! { match x { a } });
}

#[test]
fn expression_nodes() {
    let expr = check::<Expression>(quote! { foo });
    assert!(matches!(expr, Expression::WithoutBlock(_)));

    let expr_without_block = check::<ExpressionWithoutBlock>(quote! { foo });
    assert!(matches!(
        expr_without_block,
        ExpressionWithoutBlock::Path(_)
    ));

    let expr_with_block = check::<ExpressionWithBlock>(quote! { { ; } });
    assert!(matches!(expr_with_block, ExpressionWithBlock::Block(_)));

    check::<BlockExpression>(quote! { 'lbl: { ; } });
    check::<UnsafeBlockExpression>(quote! { unsafe { ; } });
    check::<LoopExpression>(quote! { 'lbl: loop { ; } });
    check::<IfExpression>(quote! { if foo { ; } else { ; } });

    let else_if = check::<ElseExpression>(quote! { if foo { ; } else { ; } });
    assert!(matches!(else_if, ElseExpression::If(_)));
    let else_block = check::<ElseExpression>(quote! { { ; } });
    assert!(matches!(else_block, ElseExpression::Block(_)));

    check::<AwaitExpression>(quote! { { ; }.await });
    check::<IndexExpression>(quote! { { ; }[idx] });
    check::<TupleExpression>(quote! { (a, b) });
    check::<ArrayExpression>(quote! { [a, b] });

    let arr_rep = check::<ArrayElements>(quote! { a; n });
    assert!(matches!(arr_rep, ArrayElements::Repetition(_, _, _)));
    let arr_list = check::<ArrayElements>(quote! { a, b, c });
    assert!(matches!(arr_list, ArrayElements::List(_)));

    check::<TupleIndexExpression>(ts("{ ; }.0"));
    check::<FieldExpression>(quote! { { ; }.field });
    check::<ReturnExpression>(quote! { return foo });
    check::<ContinueExpression>(quote! { continue 'a });
    check::<BreakExpression>(quote! { break 'a foo });
    check::<CallExpression>(quote! { { ; }(foo) });
    check::<RangeExpression>(quote! { ..b });
    check::<UnderscoreExpression>(quote! { _ });
    check::<GroupedExpression>(quote! { (foo) });
}

#[test]
fn invalid_expressions() {
    fails::<Expression>(quote! {});
    fails::<Expression>(quote! { a + });
    fails::<Expression>(quote! { f(a b) });
    fails::<Expression>(quote! { a. });
    fails::<Expression>(quote! { if a });
    fails::<Expression>(quote! { x as });
    fails::<Expression>(quote! { |x });
}
