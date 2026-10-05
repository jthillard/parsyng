# parsyng-fallback

Implementation crate of [`parsyng`](https://crates.io/crates/parsyng),
providing a pure-Rust implementation of the `proc_macro` token types, used
(with the `fallback` feature) outside of macro expansion, e.g. in tests.

You should not depend on this crate directly: add `parsyng` with the
`fallback` feature instead.

```toml
[dependencies]
parsyng = { version = "0.1", features = ["fallback"] }
```

See the [`parsyng` documentation](https://docs.rs/parsyng) and the
[repository](https://github.com/jthillard/parsyng) for more information.

## License

MIT, see [`LICENSE`](LICENSE).
