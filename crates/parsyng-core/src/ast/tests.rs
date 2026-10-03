use crate as parsyng;

use crate::{ToTokens, quote};

use crate::{
    ast::{
        crate_source::Crate,
        delimiter::{Braced, Bracketed, Parenthesized},
        expression::{
            ArrayElements, ArrayExpression, AwaitExpression, BlockExpression, BreakExpression,
            CallExpression, ContinueExpression, ElseExpression, Expression, ExpressionWithBlock,
            ExpressionWithoutBlock, FieldExpression, GroupedExpression, IfExpression,
            IndexExpression, LoopExpression, RangeExpression, ReturnExpression, TupleExpression,
            TupleIndexExpression, UnderscoreExpression, UnsafeBlockExpression,
        },
        item::{
            DeriveInput, GenericParam, GenericParams, Lifetime, LifetimeBounds, LifetimeParam,
            LifetimeWhereClauseItem, TraitBound, TypeBoundWhereClauseItem, TypeParam,
            TypeParamBound, TypeParamBounds, WhereClause, WhereClauseItem, associated::*,
            constant::ConstantItem, enum_item::EnumVariantFields, implementation::Implementation,
            r#struct::*,
        },
        literal::{
            Literal, LiteralByte, LiteralByteStr, LiteralCStr, LiteralChar, LiteralFloat,
            LiteralNumber, LiteralStr,
        },
        path::{GenericArg, GenericArgs, SimplePath, TypePathSegment},
        statements::Statement,
        r#type::{Type, TypePath},
        visibility::Visibility,
    },
    parse::ParseBuffer,
    proc_macro::TokenStream,
};

fn ts(input: &str) -> TokenStream {
    input.parse().unwrap()
}

pub fn parse_exact<T: crate::Parse>(tokens: TokenStream) -> T {
    let mut input = ParseBuffer::new(tokens);
    let value = input.parse::<T>().unwrap();
    assert!(input.is_empty());
    value
}

pub fn check<T: crate::Parse + ToTokens>(tokens: TokenStream) -> T {
    let expected = tokens.to_string();
    let parsed = parse_exact::<T>(tokens);
    let mut out = TokenStream::new();
    parsed.to_tokens(&mut out);
    assert_eq!(out.to_string(), expected);
    parsed
}

#[test]
fn token_stream_nodes() {
    check::<TokenStream>(quote! { a + b });
    check::<crate::proc_macro::TokenTree>(quote! { a });
    check::<crate::proc_macro::Group>(quote! { (a) });
    check::<crate::proc_macro::Ident>(quote! { ident });
    check::<crate::proc_macro::Punct>(quote! { + });
}

#[test]
fn delimiter_nodes() {
    check::<Bracketed<Expression>>(quote! { [foo] });
    check::<Braced<Vec<Statement>>>(quote! { { ; } });
    check::<Parenthesized<Expression>>(quote! { (foo) });
}

#[test]
fn literal_nodes() {
    let int = check::<LiteralNumber>(ts("123u32"));
    assert_eq!(int.content(), "123");
    assert_eq!(int.prefix(), "");
    assert_eq!(int.suffix(), "u32");

    let float = check::<LiteralFloat>(ts("1.5f32"));
    assert_eq!(float.content(), "1.5");
    assert_eq!(float.suffix(), "f32");

    let lit_int = check::<Literal>(ts("10"));
    assert!(matches!(lit_int, Literal::UInt(_)));

    let lit_float = check::<Literal>(ts("2.0"));
    assert!(matches!(lit_float, Literal::Float(_)));
}

#[test]
fn numeric_literal_kinds() {
    for int in ["1usize", "0x1f", "0b1010u8", "0o17", "1_000i64"] {
        assert!(
            matches!(check::<Literal>(ts(int)), Literal::UInt(_)),
            "{int}"
        );
    }
    for float in ["1e10", "1.5E-3", "2f32", "1_0.0_1f64"] {
        assert!(
            matches!(check::<Literal>(ts(float)), Literal::Float(_)),
            "{float}"
        );
    }
}

#[test]
fn string_literals() {
    let plain = check::<LiteralStr>(ts(r#""a\tb\n\"\\\x41\u{1F980}\u{0_0e9}""#));
    assert_eq!(plain.value(), "a\tb\n\"\\A\u{1F980}\u{e9}");
    assert_eq!(plain.suffix(), "");

    let raw = check::<LiteralStr>(ts("r#\"no \\n \"escapes\"\"#"));
    assert_eq!(raw.value(), r#"no \n "escapes""#);

    let continuation = check::<LiteralStr>(ts("\"a\\\n     b\""));
    assert_eq!(continuation.value(), "ab");

    let suffixed = check::<LiteralStr>(ts(r#""x"suffix"#));
    assert_eq!(suffixed.value(), "x");
    assert_eq!(suffixed.suffix(), "suffix");

    assert!(matches!(check::<Literal>(ts(r#""s""#)), Literal::Str(_)));
    assert!(matches!(check::<Literal>(ts(r#"r"s""#)), Literal::Str(_)));
    assert_eq!(parse_exact::<String>(ts(r#""hello""#)), "hello");
    // `quote!` emits plain literal tokens, so they parse directly.
    assert_eq!(parse_exact::<String>(quote! { "quoted" }), "quoted");
    assert_eq!(parse_exact::<u8>(quote! { 7 }), 7);
}

#[test]
fn byte_and_c_string_literals() {
    let bytes = check::<LiteralByteStr>(ts(r#"b"a\xFF\0""#));
    assert_eq!(bytes.value(), b"a\xFF\0");
    let raw_bytes = check::<LiteralByteStr>(ts(r##"br#"\x"#"##));
    assert_eq!(raw_bytes.value(), br"\x");
    assert!(matches!(
        check::<Literal>(ts(r#"b"s""#)),
        Literal::ByteStr(_)
    ));

    let c_str = check::<LiteralCStr>(ts(r#"c"a\xFF\u{e9}""#));
    assert_eq!(c_str.value().to_bytes(), b"a\xFF\xC3\xA9");
    let raw_c_str = check::<LiteralCStr>(ts(r#"cr"\n""#));
    assert_eq!(raw_c_str.value(), cr"\n");
    assert!(matches!(check::<Literal>(ts(r#"c"s""#)), Literal::CStr(_)));
}

#[test]
fn char_and_byte_literals() {
    assert_eq!(check::<LiteralChar>(ts("'a'")).value(), 'a');
    assert_eq!(check::<LiteralChar>(ts(r"'\''")).value(), '\'');
    assert_eq!(
        check::<LiteralChar>(ts(r"'\u{1F980}'")).value(),
        '\u{1F980}'
    );
    assert_eq!(parse_exact::<char>(ts("'z'")), 'z');
    assert!(matches!(check::<Literal>(ts("'a'")), Literal::Char(_)));

    assert_eq!(check::<LiteralByte>(ts("b'a'")).value(), b'a');
    assert_eq!(check::<LiteralByte>(ts(r"b'\xFF'")).value(), 0xFF);
    assert!(matches!(check::<Literal>(ts("b'a'")), Literal::Byte(_)));

    assert!(parse_exact::<bool>(ts("true")));
    assert!(!parse_exact::<bool>(ts("false")));
}

#[test]
fn invalid_literals_are_errors() {
    fn fails<T: crate::Parse>(input: &str) {
        let mut buffer = ParseBuffer::new(ts(input));
        assert!(buffer.parse::<T>().is_err(), "{input} should not parse");
    }
    // Malformed escapes are already rejected by the tokenizer; see the unit
    // tests in `ast::literal` for those.
    fails::<LiteralStr>("'a'");
    fails::<LiteralChar>(r#""a""#);
    fails::<LiteralByte>("'a'");
    fails::<String>("1");
    fails::<bool>("maybe");

    // A failed parse leaves the input untouched.
    let mut buffer = ParseBuffer::new(ts("'a'"));
    assert!(buffer.parse::<LiteralStr>().is_err());
    assert!(buffer.parse::<LiteralChar>().is_ok());
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

#[test]
fn item_nodes() {
    let field = check::<StructField>(quote! { pub x: u8 });
    assert_eq!(field.ident().to_string(), "x");

    let struct_struct = check::<Struct>(quote! { struct Point<T> where T: Copy { pub x: T, } });
    assert_eq!(struct_struct.ident().to_string(), "Point");
    assert!(struct_struct.generic_parameters().is_some());

    let item_struct = check::<Struct>(quote! { struct Unit; });
    assert_eq!(item_struct.ident().to_string(), "Unit");

    check::<ConstantItem>(quote! { const VALUE: u8; });
    check::<TypeAlias>(quote! { type Assoc; });

    let aa_type = check::<AssociatedAlias>(quote! { type Assoc; });
    assert!(matches!(aa_type, AssociatedAlias::TypeAlias(_, _)));
    let aa_const = check::<AssociatedAlias>(quote! { const VALUE: u8; });
    assert!(matches!(aa_const, AssociatedAlias::Const(_, _)));

    check::<Implementation>(quote! { impl Type { type Assoc; const VALUE: u8; } });
    check::<Implementation>(quote! { impl !Trait for Type { type Assoc; } });

    let item_struct = check::<crate::ast::item::Item>(quote! { pub struct S; });
    assert!(matches!(item_struct, crate::ast::item::Item::Struct(_)));
    let item_impl = check::<crate::ast::item::Item>(quote! { impl Type { type Assoc; } });
    assert!(matches!(item_impl, crate::ast::item::Item::Impl(_)));
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
