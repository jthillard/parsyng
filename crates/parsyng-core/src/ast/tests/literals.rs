use super::{check, check_str, fails, parse_exact, peek_fails, ts};
use crate as parsyng;
use crate::{
    ast::literal::{
        Literal, LiteralByte, LiteralByteStr, LiteralCStr, LiteralChar, LiteralFloat,
        LiteralNumber, LiteralStr,
    },
    parse::ParseBuffer,
};
use parsyng_quote_macros::quote;

#[test]
fn number_prefixes_and_suffixes() {
    let hex = check::<LiteralNumber>(ts("0x1Fu8"));
    assert_eq!(hex.prefix(), "0x");
    assert_eq!(hex.content(), "1F");
    assert_eq!(hex.suffix(), "u8");

    let bin = check::<LiteralNumber>(ts("0b1010"));
    assert_eq!(bin.prefix(), "0b");
    assert_eq!(bin.content(), "1010");
    assert_eq!(bin.suffix(), "");

    let oct = check::<LiteralNumber>(ts("0o17usize"));
    assert_eq!(oct.prefix(), "0o");
    assert_eq!(oct.content(), "17");
    assert_eq!(oct.suffix(), "usize");

    let exponent = check::<LiteralFloat>(ts("1e10"));
    assert_eq!(exponent.content(), "1e10");
    assert_eq!(exponent.suffix(), "");

    let suffixed = check::<LiteralFloat>(ts("2f32"));
    assert_eq!(suffixed.content(), "2");
    assert_eq!(suffixed.suffix(), "f32");
}

#[test]
fn integers_parse_to_values() {
    assert_eq!(parse_exact::<u8>(ts("7")), 7);
    assert_eq!(parse_exact::<u8>(ts("7u8")), 7);
    assert_eq!(parse_exact::<u8>(ts("0x1F")), 0x1F);
    assert_eq!(parse_exact::<u8>(ts("0b1010")), 0b1010);
    assert_eq!(parse_exact::<u16>(ts("0o17")), 0o17);
    assert_eq!(parse_exact::<u32>(ts("1_000")), 1_000);
    assert_eq!(parse_exact::<u64>(ts("0xFFFF_FFFFu64")), 0xFFFF_FFFF);
    assert_eq!(parse_exact::<usize>(ts("42usize")), 42);
    assert_eq!(
        parse_exact::<u128>(ts("340282366920938463463374607431768211455")),
        u128::MAX
    );

    // Wrong suffix, overflow, and non-integers.
    fails::<u8>(ts("7u16"));
    fails::<u8>(ts("256"));
    fails::<u8>(ts("1.0"));
    fails::<u8>(ts("\"1\""));
    fails::<u8>(ts("x"));
}

#[test]
fn literal_dispatches_on_every_kind() {
    type IsKind = fn(&Literal) -> bool;
    let cases: [(&str, IsKind); 7] = [
        ("1", |lit| matches!(lit, Literal::UInt(_))),
        ("1.0", |lit| matches!(lit, Literal::Float(_))),
        ("\"s\"", |lit| matches!(lit, Literal::Str(_))),
        ("b\"s\"", |lit| matches!(lit, Literal::ByteStr(_))),
        ("c\"s\"", |lit| matches!(lit, Literal::CStr(_))),
        ("'c'", |lit| matches!(lit, Literal::Char(_))),
        ("b'c'", |lit| matches!(lit, Literal::Byte(_))),
    ];
    for (source, is_expected) in cases {
        assert!(is_expected(&check::<Literal>(ts(source))), "{source}");
    }
    fails::<Literal>(quote! { ident });
    fails::<Literal>(quote! { - 1 });
    fails::<Literal>(quote! {});
}

#[test]
fn literal_values_with_escapes() {
    assert_eq!(check::<LiteralStr>(ts(r#""""#)).value(), "");
    assert_eq!(
        check::<LiteralStr>(ts(r###"r##"a "# b"##"###)).value(),
        r##"a "# b"##
    );
    assert_eq!(check_str::<LiteralStr>(r#""\0\r""#).value(), "\0\r");
    assert_eq!(check::<LiteralChar>(ts(r"'\n'")).value(), '\n');
    assert_eq!(check::<LiteralChar>(ts(r"'\x7F'")).value(), '\x7F');
    assert_eq!(check::<LiteralByte>(ts(r"b'\\'")).value(), b'\\');
    assert_eq!(check::<LiteralByteStr>(ts(r#"b"""#)).value(), b"");
    assert_eq!(check::<LiteralCStr>(ts(r#"c"""#)).value(), c"");
}

#[test]
fn primitive_literals_peek_without_consuming() {
    peek_fails::<String>(ts("1"));
    peek_fails::<char>(ts("\"c\""));
    peek_fails::<String>(quote! { ident });
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
