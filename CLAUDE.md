# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`parsyng` is a from-scratch replacement for `syn` + `quote` for writing Rust procedural macros: an AST with `Parse`/`ToTokens`, a `quote!` proc macro, and `#[parsyng::proc_macro]`-style helper attributes that take typed `Parse` arguments and return any `ToTokens` value. See `README.md` for the user-facing overview.

Toolchain is **nightly** (`rust-toolchain.toml`), edition 2024.

## Commands

```sh
just test                                   # parsyng-core tests with and without `full`, plus a `--no-default-features` check
cargo test -p parsyng-core --features fallback,full,extra-traits <test_name>   # single test
cargo test -p parsyng-core --features fallback,full --test parse_crates  # whole-file round-trip test
cargo test -p parsyng-fallback                # lexer tests, using proc-macro2 as an oracle
cargo clippy --workspace --all-targets --features fallback,full
just bench-runtime                          # criterion: quote!/parsing runtime vs syn+quote, unsynn, moxy
just bench-comptime                         # hyperfine: clean build of the same derive per library
just bench-quote-comptime                   # hyperfine: clean build of the same quote! template per library
just bench-expansion                        # hyperfine: expanding 200 derives + in-compiler parsing (bench-parse) per library
just bench-report                           # all of the above, then benches/report.sh writes BENCH.md (needs hyperfine, jq)
HYPERFINE_ARGS="--runs 3" just bench        # fewer hyperfine runs
```

**Tests must run with `--features fallback`** (add `full` for the whole grammar, `extra-traits` for `Debug`). Without it, token types are the compiler's real `proc_macro`, which panics outside macro expansion. This is also why many doc examples are `no_run`/`ignore`.

Every crate has `#![deny(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo, rustdoc::all, missing_docs)]` — every public item needs a doc comment, and pedantic/nursery lints are hard errors.

## Architecture

Workspace crates (root `parsyng` is a thin façade that re-exports everything):

- `crates/parsyng-core` — `ParseBuffer`/`Parse`/`Peek` (`parse.rs`), the `ToTokens` trait (`lib.rs`), combinators (`combinator.rs`: `Cons`, `Punctuated`, `Either`, `GreedyVec`, ...), diagnostics (`error.rs`), the `ast` tree, and `quote/__private` runtime helpers called by `quote!`'s expansion.
- `crates/parsyng-quote-macros` — `quote!`/`quote_spanned!` as a real proc macro (not `macro_rules!`; this is a deliberate compile-time win). Depends on nothing but `proc_macro`. It encodes each run of literal tokens (nested groups without interpolations included) into one byte-string table decoded at runtime by the non-generic, non-inlined `__private::extend_static` (format documented in `quote/__private.rs`): the expansion is constant data plus one call, which keeps release builds of big templates cheap, and the run reaches the compiler in one `extend`. Don't go back to emitting inlined per-token constructors.
- **Parallel build graph.** `parsyng-core`, `parsyng-quote-macros` and `parsyng-proc-macros` don't depend on each other (core only has quote-macros as a dev-dependency for its tests), so they compile in parallel; the façade re-exports `quote!`/`quote_spanned!` and defines `parse_quote!`. Keep it that way: any dependency between them puts a whole crate on the critical path of every user's clean build.
- Features: `parsing` (default, on both `parsyng-core` and the façade) gates `parse`, `combinator`, `ast`, `parse_quote!` and the helper attributes; without it only `quote!`/`ToTokens`/`format_ident!` remain. `full` adds expressions, statements, patterns, signatures, the `Item` enum and every item kind but structs/enums/macro invocations, and `Crate`; without it the AST is derive-level (types, paths, generics, attributes, visibility, literals, structs, enums, `DeriveInput`) and must not depend on anything behind `full` (e.g. const generic args are `path::ConstArg`, not expressions). `extra-traits` adds `Debug` on AST/combinator types (`#[cfg_attr(feature = "extra-traits", derive(Debug))]`; don't use `{:?}` on AST types in tests).
- `crates/parsyng-proc-macros` — `#[proc_macro_]`, `#[proc_macro_attribute_]`, `#[proc_macro_derive_]` (re-exported without the trailing `_` from the façade), and `#[derive(Parse)]`/`#[derive(ToTokens)]`. Deliberately standalone (no `parsyng-core` dependency): `tokens.rs` walks just the outline of a signature or struct/enum and passes every other token through with its span; output is built with `Out` (`src("...")` fragments + user tokens).
- `crates/parsyng-quote/` is a stale leftover directory, not a workspace member.

Key cross-cutting conventions:

- **`proc_macro` vs fallback switch.** `parsyng-core` hides the real `proc_macro` in a private `sealed` module and exposes `parsyng_core::proc_macro`, which is either the compiler's `proc_macro` or `parsyng_fallback` (feature `fallback`). Inside the core crate always use `crate::proc_macro::...`, never `proc_macro::...` directly, or the feature breaks. Code generated for users (helper attributes, derives) converts with `.into()` at the `proc_macro` boundary for the same reason: with `fallback` (e.g. via feature unification in `cargo test --workspace`, since `benches/runtime` enables it) those `From` impls print and re-lex.
- **`crates/parsyng-fallback`** mirrors the compiler's `proc_macro` API exactly (nothing `const`, `Punct` not `Copy`, `Extend<T: Into<TokenTree>>`), so code compiles identically against both. It never calls the compiler; tokens share their text (`Rc<str>` or `&'static str`, via the hidden `new_static_unchecked` constructors `quote!`'s decoder uses), streams are `Option<Rc<Vec<TokenTree>>>`, spans are byte ranges. Its `Display` must match `proc_macro2`'s (checked by its tests on `tests/test_files`).
- **Generated code uses unqualified `parsyng::` paths.** Both `quote!` and the derive macros emit `parsyng::proc_macro::...`, `parsyng::quote::__private::extend_static`, `parsyng::parse::ParseBuffer`, etc. Any crate that calls `quote!` must therefore have a `parsyng` name in scope: `parsyng-core`'s tests do `use crate as parsyng;` with `parsyng_quote_macros::quote` (see `ast/tests/mod.rs`).
- **Flattened buffer, free backtracking.** `ParseBuffer::new` flattens the stream once into an `Rc`-shared buffer (groups followed by their contents, ident/literal text stored once in a single string arena, keyword index precomputed). A `ParseBuffer` is a position in it: `try_parse`/`try_advance` save the position and rewind on failure, `group_contents`/`delimited` give an O(1) sub-buffer. Never re-stringify tokens in grammar code: use `peek_ident_str`, `peek_keyword`, `peek_punct_char`/`nth_punct_char`, `peek_literal_str`, `ident_str_and`. Prefer dispatching on the first token over chains of `try_parse` (see `Type::parse`). `Peek` is implemented only where matching can never consume input on failure; its `peek` method is a cheap precheck (exact for tokens) that `Option<T>`/`Punctuated` use to skip attempts that would only build and drop an error.
- **Cheap errors.** `Diagnostics` stores its first message inline and messages as `&'static str` when possible (formatting is deferred), because failed alternatives are the hot path; don't `format!` on parse failure paths.
- **Tokens.** All keywords/punctuation are type aliases over const-generic `RustKeyword<K>` / `RustPunct1/2/3`, generated by `make_tokens!` in `ast/tokens.rs`, and are referred to via `Token![...]`.
- **AST layout.** No umbrella prelude; types live at full paths (`ast::item::r#struct::Struct`, `ast::expression::IfExpression`). Common entry points are aliased in `ast::item` (`ItemStruct`, `ItemEnum`, `DeriveInput`, ...). Grammar coverage gaps are documented in the `ast.rs` module doc — unimplemented corners `todo!()`-panic rather than return errors; keep that list in sync when adding grammar.
- **Debug mode.** `#[parsyng::proc_macro(debug)]` & co. inject a call to `parsyng::debug_stream` that prints the macro output; with the `debug-pretty` feature it is piped through `rustfmt`.

## Testing pattern

Tests are round-trip checks: parse a token stream as `T`, assert the buffer is fully consumed, re-emit with `ToTokens`, and compare `to_string()` with the input (`check::<T>(quote! { ... })`). AST unit tests live in `crates/parsyng-core/src/ast/tests/` (one file per area; `mod.rs` has the `check`/`check_str`/`parse_prefix`/`fails`/`peek_fails` helpers, `rejection.rs` the must-fail inputs and documented grammar gaps) plus per-module `#[cfg(all(test, feature = "fallback"))]` blocks (`parse.rs`, `combinator.rs`, `error.rs`, `ast/token_stream.rs`, `ast/expression.rs`, `ast/pattern.rs`, ...). Gate `full`-only tests with `#[cfg(feature = "full")]` so the fallback-only run of `just test` still compiles. `tests/parse_crates.rs` round-trips every source file in `tests/test_files/` as a `Crate`, then every `fn` body in it as statements (`Crate` keeps bodies as raw tokens) — drop a new `.rs` file there to extend coverage; `syntax_showcase.rs` is a hand-written tour of the supported grammar.

Benchmarks live in `benches/`: `runtime/` (criterion; shared fixtures and an unsynn mini-grammar in `src/lib.rs`, a round-trip sanity test in `tests/`), `bench-comptime/` (one derive per library behind `syn`/`unsynn`/`moxy`/`parsyng` + `empty`/`small`/`big` features), `bench-quote-comptime/`, `bench-expansion/` (consumer of `bench-comptime`'s `HeapSize`; `gen.sh` regenerates its 200 fixture structs), and `bench-parse/` (`parse_bench!` on ~75 KB of tokio code: parsing on the compiler's real `proc_macro`; `gen.sh` regenerates it). `just profile-parse <criterion filter>` runs a parse bench under `perf`. moxy 0.5 can't parse `crate::`/`super::` paths, so whole-file parse benches use `common_subset`.

Examples under `examples/` are workspace members (each macro example is a `*-macros` proc-macro crate plus a consumer crate) and double as integration tests of the helper attributes; build them with `cargo build --workspace`.
