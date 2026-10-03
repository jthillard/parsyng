//! `quote!`/`quote_spanned!` output checks.

use parsyng_core as parsyng;
use parsyng_core::format_ident;
use parsyng_core::proc_macro::{Span, TokenStream};
use parsyng_quote_macros::{quote, quote_spanned};

/// `src` re-lexed, without whitespace: lexers disagree on the spacing of
/// some puncts (`<'a`), which doesn't change the tokens.
fn tokens(src: &str) -> String {
    strip(&src.parse::<TokenStream>().unwrap().to_string())
}

fn strip(s: &str) -> String {
    s.split_whitespace().collect()
}

#[test]
fn literal_tokens() {
    let out: TokenStream = quote! {
        impl<'de> Foo for r#Bar { fn f(&self) -> Vec<u8> { vec![1u8, 0x2, 'c' as u8] ; "s\n" ; b"x" ; r#"raw"# ; } }
        a::b::<T> += -> => ..= :: ;
    };
    assert_eq!(
        strip(&out.to_string()),
        tokens(
            r##"impl<'de> Foo for r#Bar { fn f(&self) -> Vec<u8> { vec![1u8, 0x2, 'c' as u8] ; "s\n" ; b"x" ; r#"raw"# ; } }
        a::b::<T> += -> => ..= :: ;"##
        )
    );
}

#[test]
fn interpolation() {
    let name = format_ident!("Foo");
    let n = 3u8;
    let out: TokenStream =
        quote! { struct #name([u8; #n]); const X: &str = #{ name.to_string() }; };
    assert_eq!(
        strip(&out.to_string()),
        tokens(r#"struct Foo([u8; 3u8]); const X: &str = "Foo";"#)
    );
}

#[test]
fn repetition_with_separator() {
    let names = [format_ident!("a"), format_ident!("b"), format_ident!("c")];
    let mut it = names.iter();
    let out: TokenStream = quote! { f(#(#it),*) };
    assert_eq!(strip(&out.to_string()), tokens("f(a, b, c)"));
}

#[test]
fn repetition_variable_in_nested_group() {
    let names = [format_ident!("x"), format_ident!("y")];
    let mut it = names.iter();
    let out: TokenStream = quote! { 0 #(+ size(&self.#it))* };
    assert_eq!(
        strip(&out.to_string()),
        tokens("0 + size(&self.x) + size(&self.y)")
    );

    let mut keys = names.iter();
    let mut values = [1u8, 2].into_iter();
    let out: TokenStream = quote! { #( [#keys => { #values }] )* };
    assert_eq!(
        strip(&out.to_string()),
        tokens("[x => { 1u8 }] [y => { 2u8 }]")
    );
}

#[test]
fn spanned() {
    let span = Span::call_site();
    let name = format_ident!("Foo");
    let out: TokenStream = quote_spanned! { span => impl #name { fn f() -> u8 { 1 } } };
    assert_eq!(
        strip(&out.to_string()),
        tokens("impl Foo { fn f() -> u8 { 1 } }")
    );
}
