# Errors example

This example shows how a proc-macro written with `parsyng` reports errors to its user.

A macro must never panic on bad input: it should expand to a `compile_error!` pointing at the offending tokens, so the user gets a normal compiler error. With `parsyng`, you don't have to write that by hand. A function annotated with `#[parsyng::proc_macro]`, `#[parsyng::proc_macro_attribute]` or `#[parsyng::proc_macro_derive]` can return `parsyng::error::Result<T>`, and an `Err(Diagnostics)` is expanded to one `compile_error!` per message, each spanned at the location it was created with:

```rust
#[parsyng::proc_macro]
pub fn hex_color(color: LiteralStr) -> Result<TokenStream> {
    let Some(digits) = color.value().strip_prefix('#') else {
        return Err(Diagnostics::new_error_spanned(
            "a color must start with `#`",
            color.span(),
        ));
    };
    // ...
}
```

The `errors-macros` crate shows:

- **Parse errors for free**: if the input can't be parsed as the function's argument type (`hex_color!(42)` when a `LiteralStr` is expected), the helper attribute reports the parse error itself; the function is never called.
- **Spanned errors**: `Diagnostics::new_error_spanned` points at a precise token (the string literal, the type name, a field). `Diagnostics::new_error` uses `Span::call_site()` and underlines the whole macro call instead, so use it only when nothing better is available.
- **`?` propagation**: helpers returning `parsyng::error::Result<T>` compose with `?`.
- **Several errors at once**: `#[derive(Getters)]` collects one `Diagnostic` per bad field with `Diagnostics::append` and reports them all together, instead of making the user fix them one compilation at a time.

Uncommenting the failing cases in `example/src/main.rs` and running `cargo b` gives:

```text
error: a color must start with `#`
  --> examples/errors/example/src/main.rs:13:47
   |
13 | const MISSING_HASH: (u8, u8, u8) = hex_color!("ff8800");
   |                                               ^^^^^^^^

error: expected 6 hex digits, found 4
  --> examples/errors/example/src/main.rs:14:44
   |
14 | const TOO_SHORT: (u8, u8, u8) = hex_color!("#ff88");
   |                                            ^^^^^^^

error: invalid hex digit
  --> examples/errors/example/src/main.rs:15:42
   |
15 | const NOT_HEX: (u8, u8, u8) = hex_color!("#gg0000");
   |                                          ^^^^^^^^^

error: Expected string literal
  --> examples/errors/example/src/main.rs:16:47
   |
16 | const NOT_A_STRING: (u8, u8, u8) = hex_color!(42);
   |                                               ^^

error: `Getters` can only be derived for structs
  --> examples/errors/example/src/main.rs:19:6
   |
19 | enum NotAStruct {
   |      ^^^^^^^^^^

error: `Getters` needs a struct with named fields
  --> examples/errors/example/src/main.rs:24:8
   |
24 | struct Tuple(u8);
   |        ^^^^^

error: this field already starts with `get_`, its getter would be `get_get_...`
  --> examples/errors/example/src/main.rs:28:5
   |
28 |     get_x: u8,
   |     ^^^^^

error: this field already starts with `get_`, its getter would be `get_get_...`
  --> examples/errors/example/src/main.rs:30:5
   |
30 |     get_z: u8,
   |     ^^^^^
```
