//! Implementation of the [`quote!`] and [`quote_spanned!`] procedural macros for `parsyng`.

#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    rustdoc::all,
    rustdoc::redundant_explicit_links,
    invalid_doc_attributes,
    unused_doc_comments,
    missing_docs
)]
#![allow(clippy::too_many_lines)]

use proc_macro::{Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

const INTERPOLATION_CHAR: char = '#';

/// Generates a [`proc_macro::TokenStream`] from its input while interpolating variables.
/// This works similarly to rust declarative macros, but using `#` instead of `$` for interpolation.
///
/// Interpolation is done with `#variable`, or `#{ expression }`.
/// Repetition can be done with `#(#vec),*`, where `vec` must implement the [`Iterator`] trait.
///
/// # Example
/// ```
/// use parsyng::quote;
///
/// let number = 3;
///
/// // Interpolation
/// quote! {
///     foo(#number)
/// };
///
/// let literal = "This is a string literal";
///
/// // Expression interpolation
/// quote! {
///     let uppercase = #{ literal.to_uppercase() };
/// };
///
/// let digits = vec![2, 5, 3, 1];
/// let mut digits = digits.iter();
///
/// // Repetitions
/// quote! {
///     0 #(+ #digits)*
/// };
/// ```
#[proc_macro]
pub fn quote(input: TokenStream) -> TokenStream {
    parse_tokenstream(input, false, &mut None, &mut Vec::new())
}

/// Builds a `compile_error! { ... }` token stream, used to surface errors from
/// [`quote_spanned`] at the call site instead of panicking the proc-macro.
fn make_compile_error(span: Span, inner: TokenStream) -> TokenStream {
    let mut error = TokenStream::new();
    error.extend::<[TokenTree; _]>([
        Ident::new("compile_error", span).into(),
        Punct::new('!', Spacing::Alone).into(),
        Group::new(proc_macro::Delimiter::Brace, inner).into(),
    ]);
    error
}

/// Like [`quote!`], but every generated token is built with the
/// given [`Span`] instead of [`Span::call_site`].
///
/// The input starts with a span expression, followed by `=>`, followed by the
/// same syntax accepted by [`quote!`](crate::quote).
///
/// # Example
/// ```
/// use parsyng::quote_spanned;
/// use parsyng::proc_macro::Span;
///
/// let span = Span::call_site();
/// let number = 3;
///
/// quote_spanned! {
///     span => foo(#number)
/// };
/// ```
#[proc_macro]
pub fn quote_spanned(input: TokenStream) -> TokenStream {
    let mut span = TokenStream::new();
    let mut stream = input.into_iter();
    while let Some(tt) = stream.next()
        && match tt {
            TokenTree::Punct(ref punct) => punct.as_char() != '=',
            _ => true,
        }
    {
        span.extend(core::iter::once(tt));
    }

    let tt = stream.next();
    if match tt {
        Some(TokenTree::Punct(ref punct)) => punct.as_char() != '>',
        _ => false,
    } {
        return make_compile_error(tt.clone().map_or_else(Span::call_site, |tt| tt.span()), {
            let mut tk = TokenStream::new();
            tk.extend([Literal::string(&format!(
                "expected '>', found '{}'",
                tt.map_or_else(|| "<eof>".to_string(), |tt| tt.to_string())
            ))]);
            tk
        });
    }

    let mut output = TokenStream::new();

    output.extend::<[TokenTree; _]>([
        TokenTree::Ident(Ident::new("let", Span::call_site())),
        TokenTree::Ident(Ident::new("span", Span::call_site())),
        TokenTree::Punct(Punct::new('=', Spacing::Alone)),
    ]);
    output.extend(span);
    output.extend::<[TokenTree; _]>([TokenTree::Punct(Punct::new(';', Spacing::Alone))]);

    output.extend(parse_tokenstream(
        stream.collect(),
        true,
        &mut None,
        &mut Vec::new(),
    ));

    let mut result = TokenStream::new();
    result.extend([Group::new(proc_macro::Delimiter::Brace, output)]);
    result
}

/// Walks `stream` and emits a `{ ... }` block of Rust statements that rebuild it
/// at runtime, expanding `#interpolation`, `#{expression}` and `#(...)*` as it
/// goes. The returned block evaluates to a `parsyng::proc_macro::TokenStream`
/// named `tokens`, except when called recursively for the body of a `#(...)*`
/// repetition (`in_repetition.is_some()`), in which case it only pushes into the
/// caller's `tokens` and returns no value.
///
/// `in_repetition` also doubles as the loop prologue being built for the
/// enclosing repetition: each interpolated ident used inside it gets a
/// `.next()`-and-break-on-`None` statement appended, which is why
/// `repetition_ident_already_used` exists, to avoid emitting that statement more
/// than once per ident.
fn parse_tokenstream(
    stream: TokenStream,
    span: bool,
    in_repetition: &mut Option<TokenStream>,
    repetition_ident_already_used: &mut Vec<String>,
) -> TokenStream {
    let mut output: TokenStream = TokenStream::new();

    let in_repetition_bool = in_repetition.is_some();

    if !in_repetition_bool {
        output.extend(
            "let mut tokens = parsyng::proc_macro::TokenStream::new();".parse::<TokenStream>(),
        );
    }

    let mut iter = stream.into_iter().peekable();

    while let Some(tt) = iter.next() {
        if let Some(interpolation) = match tt {
            TokenTree::Punct(ref punct)
                if punct.as_char() == INTERPOLATION_CHAR
                    && let Some(TokenTree::Ident(_)) = iter.peek() =>
            {
                let ident = iter.next().unwrap();
                match in_repetition {
                    None => Some(ident),
                    Some(loop_prologue) => {
                        if !repetition_ident_already_used.contains(&ident.to_string()) {
                            repetition_ident_already_used.push(ident.to_string());

                            let mut match_next: TokenStream = TokenStream::new();
                            let mut match_body: TokenStream = TokenStream::new();

                            // Make `Some({ident}) => {ident}, None => break`
                            match_body.extend::<[TokenTree; _]>([
                                Ident::new("Some", Span::call_site()).into(),
                                Group::new(
                                    proc_macro::Delimiter::Parenthesis,
                                    TokenStream::from(ident.clone()),
                                )
                                .into(),
                                Punct::new('=', Spacing::Joint).into(),
                                Punct::new('>', Spacing::Alone).into(),
                                ident.clone(),
                                Punct::new(',', Spacing::Alone).into(),
                                Ident::new("None", Span::call_site()).into(),
                                Punct::new('=', Spacing::Joint).into(),
                                Punct::new('>', Spacing::Alone).into(),
                                Ident::new("break", Span::call_site()).into(),
                            ]);

                            // Make `let #ident = match {ident}.next() { {match_body} };`
                            match_next.extend::<[TokenTree; _]>([
                                Ident::new("let", Span::call_site()).into(),
                                ident.clone(),
                                Punct::new('=', Spacing::Alone).into(),
                                Ident::new("match", Span::call_site()).into(),
                                ident.clone(),
                                Punct::new('.', Spacing::Alone).into(),
                                Ident::new("next", Span::call_site()).into(),
                                Group::new(proc_macro::Delimiter::Parenthesis, TokenStream::new())
                                    .into(),
                                Group::new(proc_macro::Delimiter::Brace, match_body).into(),
                                Punct::new(';', Spacing::Alone).into(),
                            ]);

                            loop_prologue.extend(match_next);
                        }

                        Some(ident)
                    }
                }
            }
            TokenTree::Punct(ref punct)
                if punct.as_char() == INTERPOLATION_CHAR
                    && let Some(TokenTree::Group(g)) = iter.peek()
                    && g.delimiter() == proc_macro::Delimiter::Brace =>
            {
                let TokenTree::Group(g) = iter.next().unwrap() else {
                    unreachable!()
                };
                Some(TokenTree::Group(Group::new(
                    proc_macro::Delimiter::None,
                    g.stream(),
                )))
            }
            TokenTree::Punct(ref punct)
                if punct.as_char() == INTERPOLATION_CHAR
                    && let Some(TokenTree::Group(g)) = iter.peek()
                    && g.delimiter() == proc_macro::Delimiter::Parenthesis =>
            {
                if in_repetition_bool {
                    return make_compile_error(g.span(), {
                        let mut tk = TokenStream::new();
                        tk.extend([Literal::string(
                            "Quote repetition inside another repetition is forbidden",
                        )]);
                        tk
                    });
                }
                let TokenTree::Group(g) = iter.next().unwrap() else {
                    unreachable!()
                };

                let mut first_loop = TokenStream::new();

                // Append tokens after #(...) to the first_loop until the first *
                while let Some(tt) = match iter.next() {
                    Some(TokenTree::Punct(punct)) => {
                        if punct.as_char() == '*' {
                            None
                        } else {
                            Some(TokenTree::Punct(punct))
                        }
                    }
                    tt => tt,
                } {
                    token_to_construction_code(
                        &mut first_loop,
                        tt,
                        span,
                        in_repetition,
                        repetition_ident_already_used,
                    );
                }

                let mut loop_prologue = Some(TokenStream::new());

                let body = parse_tokenstream(g.stream(), span, &mut loop_prologue, &mut Vec::new());

                let mut loop_body = loop_prologue.unwrap();

                // Make `if __quote_first {  } __quote_first = false;`
                loop_body.extend::<[TokenTree; _]>([
                    Ident::new("if", Span::call_site()).into(),
                    Punct::new('!', Spacing::Alone).into(),
                    Ident::new("__quote_first", Span::call_site()).into(),
                    Group::new(proc_macro::Delimiter::Brace, first_loop).into(),
                    Ident::new("__quote_first", Span::call_site()).into(),
                    Punct::new('=', Spacing::Alone).into(),
                    Ident::new("false", Span::call_site()).into(),
                    Punct::new(';', Spacing::Alone).into(),
                ]);

                loop_body.extend(body);

                // Make `let __quote_first = true; loop { {body} }`
                output.extend::<[TokenTree; _]>([
                    Ident::new("let", Span::call_site()).into(),
                    Ident::new("mut", Span::call_site()).into(),
                    Ident::new("__quote_first", Span::call_site()).into(),
                    Punct::new('=', Spacing::Alone).into(),
                    Ident::new("true", Span::call_site()).into(),
                    Punct::new(';', Spacing::Alone).into(),
                    Ident::new("loop", Span::call_site()).into(),
                    Group::new(proc_macro::Delimiter::Brace, loop_body).into(),
                ]);

                continue;
            }
            _ => None,
        } {
            let mut args = TokenStream::new();

            // Make `&{interpolation}, &mut tokens`
            args.extend::<[TokenTree; _]>([
                Punct::new('&', Spacing::Alone).into(),
                interpolation,
                Punct::new(',', Spacing::Alone).into(),
                Punct::new('&', Spacing::Alone).into(),
                Ident::new("mut", Span::call_site()).into(),
                Ident::new("tokens", Span::call_site()).into(),
            ]);

            // Make `::parsyng::ToTokens::to_tokens({args});`
            output.extend::<[TokenTree; _]>([
                Ident::new("parsyng", Span::call_site()).into(),
                Punct::new(':', Spacing::Joint).into(),
                Punct::new(':', Spacing::Alone).into(),
                Ident::new("ToTokens", Span::call_site()).into(),
                Punct::new(':', Spacing::Joint).into(),
                Punct::new(':', Spacing::Alone).into(),
                Ident::new("to_tokens", Span::call_site()).into(),
                Group::new(proc_macro::Delimiter::Parenthesis, args).into(),
                Punct::new(';', Spacing::Alone).into(),
            ]);
        } else {
            token_to_construction_code(
                &mut output,
                tt,
                span,
                in_repetition,
                repetition_ident_already_used,
            );
        }
    }

    if !in_repetition_bool {
        output.extend(core::iter::once(Ident::new("tokens", Span::call_site())));
    }

    TokenTree::Group(Group::new(proc_macro::Delimiter::Brace, output)).into()
}

/// Emits the statement(s) that push a single non-interpolated `tt` onto
/// `tokens`, calling the matching `parsyng::quote::__private::push_*` helper
/// for its kind (group, ident, punct or literal). Groups recurse through
/// [`parse_tokenstream`] to build their own contents first.
fn token_to_construction_code(
    output: &mut TokenStream,
    tt: TokenTree,
    spanned: bool,
    in_repetition: &mut Option<TokenStream>,
    repetition_ident_already_used: &mut Vec<String>,
) {
    let spanned_fn = if spanned { "_spanned" } else { "" };
    let spanned_arg = if spanned { "span.clone(), " } else { "" };
    match tt {
        TokenTree::Group(group) => {
            let inner = parse_tokenstream(
                group.stream(),
                spanned,
                in_repetition,
                repetition_ident_already_used,
            );

            let f =
                format!("parsyng::quote::__private::push_group{spanned_fn}").parse::<TokenStream>();

            let mut args = format!("parsyng::proc_macro::Delimiter::{:?}, ", group.delimiter())
                .parse::<TokenStream>()
                .unwrap();

            args.extend(inner);

            args.extend(format!(", {spanned_arg}&mut tokens").parse::<TokenStream>());

            let args = TokenTree::Group(Group::new(proc_macro::Delimiter::Parenthesis, args));

            output.extend(f);
            output.extend(Some(args));
            output.extend(Some(Punct::new(';', Spacing::Alone)));
        }
        TokenTree::Ident(ident) => {
            let ident_string = ident.to_string();
            if let Some(raw_ident) = ident_string.strip_prefix("r#") {
                output.extend(
                    format!(
                        "parsyng::quote::__private::push_ident_raw{spanned_fn}(\"{raw_ident}\", {spanned_arg}&mut tokens);",
                    )
                    .parse::<TokenStream>(),
                );
            } else {
                output.extend(
                    format!(
                        "parsyng::quote::__private::push_ident{spanned_fn}(\"{ident_string}\", {spanned_arg}&mut tokens);",
                    )
                    .parse::<TokenStream>(),
                );
            }
        }
        TokenTree::Punct(punct) => match punct.spacing() {
            Spacing::Joint => output.extend(
                format!(
                    "parsyng::quote::__private::push_punct_joint{}('{}', {}&mut tokens);",
                    spanned_fn,
                    punct.as_char().escape_default(),
                    spanned_arg,
                )
                .parse::<TokenStream>(),
            ),
            Spacing::Alone => output.extend(
                format!(
                    "parsyng::quote::__private::push_punct_alone{}('{}', {}&mut tokens);",
                    spanned_fn,
                    punct.as_char().escape_default(),
                    spanned_arg,
                )
                .parse::<TokenStream>(),
            ),
        },
        TokenTree::Literal(literal) => {
            let literal = literal.to_string();
            let literal_escaped = literal.escape_default();
            output.extend(
                format!(
                    "parsyng::quote::__private::push_lit{spanned_fn}(\"{literal_escaped}\".parse::<parsyng::proc_macro::TokenStream>().unwrap(), {spanned_arg}&mut tokens);",
                )
                .parse::<TokenStream>(),
            );
        }
    }
}
