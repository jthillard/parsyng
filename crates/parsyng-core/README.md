# parsyng-core

Implementation crate of [`parsyng`](https://crates.io/crates/parsyng),
providing the parser (`ParseBuffer`, `Parse`, `Peek`), the `ToTokens` trait, combinators, diagnostics, the `ast` syntax tree and the runtime helpers used by `quote!`.

You should not depend on this crate directly: add `parsyng` instead, which
re-exports everything from here.

```toml
[dependencies]
parsyng = "0.1"
```

See the [`parsyng` documentation](https://docs.rs/parsyng) and the
[repository](https://github.com/supersurviveur/parsyng) for more information.

## License

MIT, see [`LICENSE`](LICENSE).
