#![cfg(feature = "full")]
use std::fs;

use parsyng_core::ast::crate_source::Crate;
use parsyng_core::proc_macro::TokenStream;
mod utils;
use utils::check;

#[test]
fn test_full_crate_parse() {
    let mut files: Vec<_> = fs::read_dir("tests/test_files")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    assert!(!files.is_empty());
    for file in files {
        // Printed (and shown by the test harness) if this file fails.
        println!("checking {}", file.display());
        let source = fs::read_to_string(&file).unwrap();
        let tokens: TokenStream = source.parse().unwrap();
        check::<Crate>(tokens);
    }
}
