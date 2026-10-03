#![cfg(feature = "full")]
use std::fs;

use parsyng_core::ast::crate_source::Crate;
use parsyng_core::proc_macro::TokenStream;
mod utils;
use utils::check;

#[test]
fn test_full_crate_parse() {
    for file in fs::read_dir("tests/test_files").unwrap() {
        let file = file.unwrap().path();
        if file.is_file() {
            let source = fs::read_to_string(file).unwrap();
            let tokens: TokenStream = source.parse().unwrap();

            check::<Crate>(tokens);
        }
    }
}
