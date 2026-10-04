//! Lexer tests, using `proc_macro2` as an oracle: both must print the same
//! tokens for the same input.

use parsyng_fallback::{Delimiter, Ident, Literal, Spacing, Span, TokenStream, TokenTree};

fn lex(src: &str) -> TokenStream {
    src.parse()
        .unwrap_or_else(|e| panic!("failed to lex {src:?}: {e}"))
}

/// Same printed tokens as `proc_macro2`, and printing then re-lexing is
/// stable.
fn same_as_proc_macro2(src: &str) {
    let ours = lex(src).to_string();
    let theirs = src.parse::<proc_macro2::TokenStream>().unwrap().to_string();
    assert_eq!(ours, theirs, "on {src:?}");
    assert_eq!(lex(&ours).to_string(), ours, "re-lexing {src:?}");
}

#[test]
fn real_files() {
    for src in [
        include_str!("../../parsyng-core/tests/test_files/broadcast.rs"),
        include_str!("../../parsyng-core/tests/test_files/delay_queue.rs"),
        include_str!("../../parsyng-core/tests/test_files/entry.rs"),
        include_str!("../../parsyng-core/tests/test_files/local.rs"),
    ] {
        same_as_proc_macro2(src);
    }
}

#[test]
fn token_kinds() {
    for src in [
        "a _b r#fn r#type über",
        "'a 'static 'r#loop: loop {} &'a T",
        "'x' '\\'' '\\n' '\\u{1F600}' '\"' 'é' b'a' b'\\x7f' b'\\''",
        r##""" "a\"b" "multi
line" b"bytes\0" c"cstr" r"raw" r#"raw "quoted""# br"x" br#"y"# cr"z" "suffix"sfx"##,
        "1 1u8 1_000 0x1F 0xffu8 0o17 0b1010_1010 1.5 1.5e10 1e-3 1E+5_f32 2.f64 1. 1.0.max",
        "1..2 1..=2 x.0.1 tup.0 1.max(2) 3usize",
        "+ += -> => :: ::< <<= ... ..= # #! $ @ ~ ? ; , . / /= % ^ & | ! != == <= >=",
        "a // line comment\n b /* block /* nested */ comment */ c",
        "/// outer doc\nfn f() {}\n//! inner doc\n/** block doc */ /*! inner block */ //// not doc\n/*** not doc */ /**/",
        "(a [b {c}] {}) ( ) [ ]",
        "/x //y\n- /*z*/ -",
    ] {
        same_as_proc_macro2(src);
    }
}

#[test]
fn spans_are_byte_ranges() {
    let stream = lex("fn  foo(x)");
    let trees: Vec<_> = stream.into_iter().collect();
    assert_eq!(trees[0].span().byte_range(), 0..2);
    assert_eq!(trees[1].span().byte_range(), 4..7);
    let TokenTree::Group(group) = &trees[2] else {
        panic!("expected a group")
    };
    assert_eq!(group.span().byte_range(), 7..10);
    assert_eq!(group.span_open().byte_range(), 7..8);
    assert_eq!(group.span_close().byte_range(), 9..10);
    assert_eq!(Span::call_site().byte_range(), 0..0);
}

#[test]
fn spacing_and_lifetimes() {
    let trees: Vec<_> = lex("+= &'a").into_iter().collect();
    let spacings: Vec<_> = trees
        .iter()
        .filter_map(|tree| match tree {
            TokenTree::Punct(punct) => Some((punct.as_char(), punct.spacing())),
            _ => None,
        })
        .collect();
    assert_eq!(
        spacings,
        [
            ('+', Spacing::Joint),
            ('=', Spacing::Alone),
            ('&', Spacing::Joint),
            ('\'', Spacing::Joint)
        ]
    );
}

#[test]
fn errors() {
    for src in [
        "(",
        ")",
        "(]",
        "\"unterminated",
        "/* unterminated",
        "'",
        "\u{1F600}",
        "r#\"x\"",
        "'ab'",
    ] {
        assert!(
            src.parse::<TokenStream>().is_err(),
            "{src:?} should not lex"
        );
    }
    let error = "a (b ]".parse::<TokenStream>().unwrap_err();
    assert_eq!(error.span().byte_range(), 5..6);
}

#[test]
fn literals() {
    for (literal, expected) in [
        (Literal::u8_suffixed(1), "1u8"),
        (Literal::usize_unsuffixed(42), "42"),
        (Literal::i32_unsuffixed(-3), "-3"),
        (Literal::f64_unsuffixed(1.0), "1.0"),
        (Literal::f32_suffixed(1.5), "1.5f32"),
        (Literal::string("a\"b\n'\0"), "\"a\\\"b\\n'\\0\""),
        (Literal::character('\''), "'\\''"),
        (Literal::character('"'), "'\"'"),
        (Literal::byte_character(b'\''), "b'\\''"),
        (Literal::byte_string(b"a\x00\xff"), "b\"a\\0\\xFF\""),
        (Literal::c_string(c"hi"), "c\"hi\""),
    ] {
        assert_eq!(literal.to_string(), expected);
        let theirs: proc_macro2::Literal = expected.parse().unwrap();
        assert_eq!(theirs.to_string(), expected);
    }
    assert_eq!("-1.5".parse::<Literal>().unwrap().to_string(), "-1.5");
    assert_eq!("\"s\"".parse::<Literal>().unwrap().to_string(), "\"s\"");
    for invalid in ["", "a", "1 2", "-", "- a", "(1)"] {
        assert!(
            invalid.parse::<Literal>().is_err(),
            "{invalid:?} is not a literal"
        );
    }
}

#[test]
fn streams() {
    let mut stream = TokenStream::new();
    assert!(stream.is_empty());
    stream.extend([TokenTree::from(Ident::new("a", Span::call_site()))]);
    let copy = stream.clone();
    stream.extend(lex("+ b"));
    assert_eq!(copy.to_string(), "a");
    assert_eq!(stream.to_string(), "a + b");
    let group = parsyng_fallback::Group::new(Delimiter::Brace, stream.clone());
    assert_eq!(
        TokenStream::from(TokenTree::from(group)).to_string(),
        "{ a + b }"
    );
    assert_eq!(Ident::new("r#fn", Span::call_site()).to_string(), "r#fn");
    let collected: TokenStream = [stream.clone(), TokenStream::new(), stream]
        .into_iter()
        .collect();
    assert_eq!(collected.to_string(), "a + b a + b");
}

#[test]
#[should_panic = "not a valid identifier"]
fn invalid_ident() {
    let _ = Ident::new("1a", Span::call_site());
}
