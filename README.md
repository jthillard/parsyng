# parsyng

`parsyng` is a toolkit for writing Rust procedural macros: a parser for Rust
syntax plus a token-stream builder, filling the role that [`syn`] and
[`quote`] fill together, built from scratch as a single, dependency-light
crate.

[`syn`]: https://docs.rs/syn
[`quote`]: https://docs.rs/quote

```toml
[dependencies]
parsyng = "0.1"
```

## What's in the box

- **`ast`** — a tree of Rust syntax types (items, expressions, types,
  patterns, generics, ...), each implementing `Parse` to turn a
  `TokenStream` into a typed value and `ToTokens` to turn it back into one.
- **`quote!`** and **`quote_spanned!`** — build a token stream from
  almost-literal Rust syntax, interpolating `#variable`s and repeating
  `#(#items),*` sequences, the same way the `quote` crate does.
- **`#[parsyng::proc_macro]`**, **`#[parsyng::proc_macro_attribute]`** and
  **`#[parsyng::proc_macro_derive]`** — drop-in replacements for
  `#[proc_macro]`, `#[proc_macro_attribute]` and `#[proc_macro_derive]` that
  let the annotated function take typed, `Parse`-implementing arguments and
  return any `ToTokens` value, instead of hand-rolling `TokenStream` parsing
  and error reporting.
- **`#[derive(Parse)]`** and **`#[derive(ToTokens)]`** — implement both traits
  on your own structs field-by-field.

## Quick start

A minimal function-like macro that doubles an integer literal:

```rust
// in a crate with `[lib] proc-macro = true`
#[parsyng::proc_macro]
pub fn double(n: u32) -> u32 {
    n * 2
}
```

```rust,ignore
// in a crate depending on the macro crate above
assert_eq!(double!(21), 42);
```

The attribute parses `n` out of the macro's input using `Parse` (erroring out
with a `compile_error!` if that fails), calls the function body, and turns the
returned `u32` back into tokens with `ToTokens` — no manual `TokenStream`
plumbing required.

A `#[derive(...)]`-style macro built directly on the `ast` types, ported from
`syn`'s own `heapsize` example, lives in [`examples/heapsize`](examples/heapsize).

## Why not `syn`?

`parsyng` does not aim to be a superset of `syn`'s grammar coverage — a few
corners (string/char/byte literals, slice/range/box patterns, unstable
let-chains, `try`/`yeet` blocks, inline `asm!`) are still unimplemented; see
the `ast` module documentation for exact coverage. What it offers instead:

- A single crate with no required dependency on `syn`/`quote`, built directly
  on `proc_macro` (or, optionally, `proc_macro2`).
- `quote!` implemented as a genuine procedural macro rather than a
  `macro_rules!`, which noticeably reduces the compile time of macro-heavy
  crates — see [`benches/`](benches) for comparative numbers.
- The `#[parsyng::proc_macro]` / `#[parsyng::proc_macro_attribute]` /
  `#[parsyng::proc_macro_derive]` helper attributes, which remove almost all
  of the boilerplate `syn`/`quote`-based macros still need to hand-write
  (parsing the input, matching on the `Result`, converting the output).

## Feature flags

- **`proc-macro2`** — use the [`proc_macro2`](https://docs.rs/proc-macro2)
  crate instead of the compiler's built-in `proc_macro` for every token type.
  Required to call `quote!`, `parse_quote!` or any `Parse`/`ToTokens`
  implementation outside of an actual macro invocation (for example, in unit
  tests or a `build.rs`), since the real `proc_macro` crate panics when used
  outside the compiler's macro expansion context.
- **`debug-pretty`** — when a macro built with `#[parsyng::proc_macro]` & co.
  is annotated with the `debug` argument (e.g.
  `#[parsyng::proc_macro(debug)]`), pipe its generated output through
  `rustfmt` before printing it, instead of printing the raw, unformatted
  token stream. See [`examples/debug-attribute`](examples/debug-attribute)
  for why this is useful when a macro emits invalid syntax that the Rust
  parser itself can't explain.

## Crate layout

This `parsyng` crate is a thin façade over three implementation crates,
all re-exported here so that depending on `parsyng` alone is enough:

| Crate                 | Provides                                                              |
| ---------------------- | ---------------------------------------------------------------------- |
| `parsyng-core`         | `ast`, `parse`, `combinator`, `error`, the `ToTokens` trait           |
| `parsyng-quote-macros` | `quote!`, `quote_spanned!`                                             |
| `parsyng-proc-macros`  | `#[proc_macro]` & co., `#[derive(Parse)]`, `#[derive(ToTokens)]`      |

## Examples

- [`examples/simple-use`](examples/simple-use) — a grab bag exercising all
  three macro-helper attributes plus the `Parse`/`ToTokens` derives.
- [`examples/heapsize`](examples/heapsize) — a `#[derive(HeapSize)]` macro
  walking struct fields, ported from `syn`'s documentation.
- [`examples/debug-attribute`](examples/debug-attribute) — using the `debug`
  argument to diagnose a macro that emits invalid Rust syntax.

## License

MIT, see [`LICENSE`](LICENSE).
