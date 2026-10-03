# parsyng-proc-macros

Implementation crate of [`parsyng`](https://crates.io/crates/parsyng),
providing the `#[proc_macro]`, `#[proc_macro_attribute]` and `#[proc_macro_derive]` helper attributes and the `#[derive(Parse)]` / `#[derive(ToTokens)]` derives.

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
