#![cfg(feature = "full")]
use std::fs;

use parsyng_core::ast::{crate_source::Crate, delimiter::Braced, statements::Statement};
use parsyng_core::proc_macro::{Delimiter, TokenStream, TokenTree};
mod utils;
use utils::check;

/// Every brace-delimited `fn` body in `tokens` (nested ones included).
///
/// `Crate` keeps function bodies as raw tokens, so they are found by a token
/// scan: the first `{...}` group after a `fn` keyword, unless a `;` (a
/// declaration, or a `fn(..)` pointer type) comes first.
fn function_bodies(tokens: TokenStream, out: &mut Vec<TokenTree>) {
    let mut in_signature = false;
    for tree in tokens {
        match &tree {
            TokenTree::Ident(ident) if ident.to_string() == "fn" => in_signature = true,
            TokenTree::Punct(punct) if punct.as_char() == ';' => in_signature = false,
            TokenTree::Group(group) => {
                if in_signature && group.delimiter() == Delimiter::Brace {
                    in_signature = false;
                    out.push(tree.clone());
                }
                function_bodies(group.stream(), out);
            }
            _ => {}
        }
    }
}

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

        check::<Crate>(tokens.clone());

        let mut bodies = Vec::new();
        function_bodies(tokens, &mut bodies);
        assert!(!bodies.is_empty());
        for body in bodies {
            println!("checking function body `{body}`");
            check::<Braced<Vec<Statement>>>(body.into());
        }
    }
}
