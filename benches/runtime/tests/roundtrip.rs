//! Sanity check for the parse benches: on the common subset of every file,
//! syn, parsyng and moxy must all re-emit the same tokens they parsed.

use parsyng_bench_runtime::{FILES, common_subset};
use parsyng::ast::crate_source::Crate;

/// Normalise token spacing by re-lexing through proc-macro2.
fn normalize(src: &str) -> String {
    src.parse::<proc_macro2::TokenStream>().unwrap().to_string()
}

#[test]
fn parsers_roundtrip_common_subset() {
    for &(name, src) in FILES {
        let subset = common_subset(src);
        let expected = normalize(&subset);

        let syn_out = quote::ToTokens::into_token_stream(syn::parse_file(&subset).unwrap());
        assert_eq!(normalize(&syn_out.to_string()), expected, "syn on {name}");

        let parsyng_out = parsyng::ToTokens::to_token_stream(
            &parsyng::parse::parse_all::<Crate>(subset.parse().unwrap()).unwrap(),
        );
        assert_eq!(normalize(&parsyng_out.to_string()), expected, "parsyng on {name}");

        let moxy_file: moxy::ast::File = moxy::parse!(subset).unwrap();
        let moxy_out = moxy::token::ToTokenStream::to_token_stream(&moxy_file);
        // moxy drops `Spacing::Joint` inside macro bodies (`<>` -> `< >`), so
        // only compare it modulo whitespace.
        let strip = |s: &str| s.split_whitespace().collect::<String>();
        assert_eq!(strip(&moxy_out.to_string()), strip(&expected), "moxy on {name}");
    }
}
