//! The same derive macros written with syn+quote, unsynn, moxy and parsyng.
//!
//! Exactly one library feature (`syn`, `unsynn`, `moxy`, `parsyng`) and one
//! size feature must be enabled:
//! - `empty`: the library is a dependency, but no macro uses it.
//! - `small`: `#[derive(Named)]`, parse the input and emit a one-line impl.
//! - `big`: `#[derive(HeapSize)]`, a heapsize-style derive with generics
//!   handling and per-field repetition (also used by `bench-expansion`).
//! - `parse-bench`: `parse_bench!{ ... }` parses its input as a whole source
//!   file and emits nothing, to time parsing on the compiler's real
//!   `proc_macro` (used by `bench-parse`; not available with unsynn, which
//!   has no Rust grammar).
//!
//! Generated impls refer to `crate::HeapSize`, so the consumer crate defines
//! the trait at its root.

// Each feature combination only uses part of the per-library modules.
#![allow(unused_imports, dead_code)]

use proc_macro::TokenStream;

#[cfg(any(feature = "small", feature = "big", feature = "parse-bench"))]
cfg_select! {
    feature = "syn" => {
        mod syn_impl;
        use syn_impl as imp;
    }
    feature = "unsynn" => {
        mod unsynn_impl;
        use unsynn_impl as imp;
    }
    feature = "moxy" => {
        mod moxy_impl;
        use moxy_impl as imp;
    }
    feature = "parsyng" => {
        mod parsyng_impl;
        use parsyng_impl as imp;
    }
}

/// Only exists so the crate exports something in the `empty` case.
#[cfg(not(any(feature = "small", feature = "big", feature = "parse-bench")))]
#[proc_macro]
pub fn macro_bench(_: TokenStream) -> TokenStream {
    TokenStream::new()
}

/// `impl Name { pub const NAME: &str = "Name"; }`
#[cfg(feature = "small")]
#[proc_macro_derive(Named)]
pub fn derive_named(input: TokenStream) -> TokenStream {
    imp::named(input)
}

/// `impl crate::HeapSize for Name { ... }`, summing the heap size of every field.
#[cfg(feature = "big")]
#[proc_macro_derive(HeapSize)]
pub fn derive_heap_size(input: TokenStream) -> TokenStream {
    imp::heap_size(input)
}

/// Parse the input as a whole source file, and emit nothing.
#[cfg(feature = "parse-bench")]
#[proc_macro]
pub fn parse_bench(input: TokenStream) -> TokenStream {
    imp::parse_file(input)
}
