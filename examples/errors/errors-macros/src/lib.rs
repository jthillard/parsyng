use parsyng::{
    ast::{
        item::{DeriveInput, r#struct::StructFields},
        literal::LiteralStr,
    },
    error::{Diagnostic, Diagnostics, Result},
    proc_macro::{Ident, Span, TokenStream},
    quote,
};

/// `hex_color!("#ff8800")` expands to the tuple `(255u8, 136u8, 0u8)`.
///
/// Two kinds of errors can happen here:
/// - the input is not a string literal (`hex_color!(42)`): parsing the
///   `LiteralStr` argument fails, and the helper attribute turns that into a
///   `compile_error!` before this function is even called;
/// - the input is a string, but not a valid color: this function returns an
///   `Err`, which is expanded to a `compile_error!` as well.
#[parsyng::proc_macro]
pub fn hex_color(color: LiteralStr) -> Result<TokenStream> {
    // Every error is spanned at the literal, so the compiler underlines the
    // offending string instead of the whole macro call.
    let span = color.span();
    let Some(digits) = color.value().strip_prefix('#') else {
        return Err(Diagnostics::new_error_spanned(
            "a color must start with `#`",
            span,
        ));
    };
    if digits.len() != 6 {
        // A `String` message, built with `format!`: use a `&'static str`
        // whenever the message doesn't depend on the input.
        return Err(Diagnostics::new_error_spanned(
            format!("expected 6 hex digits, found {}", digits.len()),
            span,
        ));
    }

    // `?` propagates the first channel that fails to parse.
    let r = channel(&digits[0..2], span)?;
    let g = channel(&digits[2..4], span)?;
    let b = channel(&digits[4..6], span)?;
    Ok(quote! { (#r, #g, #b) })
}

/// Parse one two-digit hexadecimal color channel.
fn channel(digits: &str, span: Span) -> Result<u8> {
    u8::from_str_radix(digits, 16)
        .map_err(|_| Diagnostics::new_error_spanned("invalid hex digit", span))
}

/// `#[derive(Getters)]` generates a `get_<field>(&self) -> &Type` method for
/// every field of a struct with named fields.
#[parsyng::proc_macro_derive(Getters)]
pub fn derive_getters(input: DeriveInput) -> Result<TokenStream> {
    // Point at the type's name: an error spanned at `Span::call_site()`
    // (what `Diagnostics::new_error` does) would underline the whole
    // `#[derive(...)]` instead.
    let DeriveInput::Struct(ref item) = input else {
        return Err(Diagnostics::new_error_spanned(
            "`Getters` can only be derived for structs",
            input.ident().span(),
        ));
    };
    let StructFields::Named(ref fields) = item.fields else {
        return Err(Diagnostics::new_error_spanned(
            "`Getters` needs a struct with named fields",
            input.ident().span(),
        ));
    };
    let fields = fields.inner_ref();

    // Instead of stopping at the first bad field, collect one diagnostic per
    // field, so the user can fix all of them in one go.
    let mut errors: Option<Diagnostics> = None;
    for field in fields.iter() {
        if field.ident().to_string().starts_with("get_") {
            let diagnostic = Diagnostic::new(
                "this field already starts with `get_`, its getter would be `get_get_...`",
                field.span(),
            );
            match errors {
                Some(ref mut errors) => errors.append(diagnostic),
                None => errors = Some(Diagnostics::new(diagnostic)),
            }
        }
    }
    if let Some(errors) = errors {
        return Err(errors);
    }

    let mut getters = fields.iter().map(|field| {
        let ident = field.ident();
        let ty = field.ty();
        // Give the getter the field's span, so errors in the generated code
        // point at the field it comes from.
        let getter = Ident::new(&format!("get_{ident}"), ident.span());
        quote! {
            pub fn #getter(&self) -> &#ty {
                &self.#ident
            }
        }
    });

    let (impl_generics, ty_generics, where_clause) = input.split_generics_for_impl();
    Ok(quote! {
        impl #impl_generics #{ input.ident() } #ty_generics #where_clause {
            #(#getters)*
        }
    })
}
