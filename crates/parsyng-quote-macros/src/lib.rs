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

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

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
    parse_tokenstream(input, false)
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

    output.extend(parse_tokenstream(stream.collect(), true));

    let mut result = TokenStream::new();
    result.extend([Group::new(proc_macro::Delimiter::Brace, output)]);
    result
}

/// One piece of `quote!`'s output.
enum Item {
    /// Literal tokens (groups without interpolations included), already
    /// encoded for `__private::extend_static`. Consecutive runs are merged
    /// into a single call.
    Static(Vec<u8>),
    /// An expression evaluating to a single `TokenTree` (a group containing
    /// interpolations).
    Tree(TokenStream),
    /// Statements pushing into `tokens` (interpolations, repetitions).
    Code(TokenStream),
}

/// The state of an enclosing `#(...)*` repetition: its loop prologue (one
/// `let x = match x.next() { .. }` per interpolated variable) and the
/// variables already in it.
struct Repetition {
    prologue: TokenStream,
    used: Vec<String>,
}

fn ident(name: &str) -> TokenTree {
    Ident::new(name, Span::call_site()).into()
}

fn punct(ch: char) -> TokenTree {
    Punct::new(ch, Spacing::Alone).into()
}

fn joint(ch: char) -> TokenTree {
    Punct::new(ch, Spacing::Joint).into()
}

fn group(delimiter: Delimiter, stream: TokenStream) -> TokenTree {
    Group::new(delimiter, stream).into()
}

/// `parsyng :: <segments> :: ...`
fn path(out: &mut TokenStream, segments: &[&str]) {
    out.extend([ident("parsyng")]);
    for segment in segments {
        out.extend([joint(':'), punct(':'), ident(segment)]);
    }
}

/// `parsyng::quote::__private::<helper>(<args>)`
fn call_private(helper: &str, args: TokenStream) -> TokenStream {
    let mut out = TokenStream::new();
    path(&mut out, &["quote", "__private", helper]);
    out.extend([group(Delimiter::Parenthesis, args)]);
    out
}

/// `<arg>, span` when spanned, `<arg>` otherwise.
fn with_span(mut args: TokenStream, spanned: bool) -> TokenStream {
    if spanned {
        args.extend([punct(','), ident("span")]);
    }
    args
}

/// `let mut tokens = parsyng::proc_macro::TokenStream::new();`
fn new_tokens() -> TokenStream {
    let mut out = TokenStream::new();
    out.extend([ident("let"), ident("mut"), ident("tokens"), punct('=')]);
    path(&mut out, &["proc_macro", "TokenStream", "new"]);
    out.extend([
        group(Delimiter::Parenthesis, TokenStream::new()),
        punct(';'),
    ]);
    out
}

/// `&mut tokens, <rest>`
fn tokens_and(rest: impl IntoIterator<Item = TokenTree>) -> TokenStream {
    let mut args = TokenStream::new();
    args.extend([punct('&'), ident("mut"), ident("tokens"), punct(',')]);
    args.extend(rest);
    args
}

/// The statements appending `items` to `tokens`.
fn emit(items: Vec<Item>, spanned: bool) -> TokenStream {
    let mut out = TokenStream::new();
    let mut run = Vec::new();
    let flush = |out: &mut TokenStream, run: &mut Vec<u8>| {
        if run.is_empty() {
            return;
        }
        // `parsyng::quote::__private::extend_static(&mut tokens, b"<ops>");`
        let ops = TokenTree::Literal(Literal::byte_string(&core::mem::take(run)));
        let helper = if spanned {
            "extend_static_spanned"
        } else {
            "extend_static"
        };
        out.extend(call_private(helper, with_span(tokens_and([ops]), spanned)));
        out.extend([punct(';')]);
    };
    for item in items {
        match item {
            Item::Static(ops) => run.extend_from_slice(&ops),
            Item::Tree(tree) => {
                flush(&mut out, &mut run);
                // `parsyng::quote::__private::push(&mut tokens, <tree>);`
                out.extend(call_private("push", tokens_and(tree)));
                out.extend([punct(';')]);
            }
            Item::Code(code) => {
                flush(&mut out, &mut run);
                out.extend(code);
            }
        }
    }
    flush(&mut out, &mut run);
    out
}

/// A block evaluating to the `TokenStream` described by `items`.
fn stream_expression(items: Vec<Item>, spanned: bool) -> TokenStream {
    let mut block = new_tokens();
    block.extend(emit(items, spanned));
    block.extend([ident("tokens")]);
    group(Delimiter::Brace, block).into()
}

/// Walks `stream` and returns a `{ ... }` block of Rust statements that
/// rebuild it at runtime, expanding `#interpolation`, `#{expression}` and
/// `#(...)*`, and evaluating to a `parsyng::proc_macro::TokenStream`.
fn parse_tokenstream(stream: TokenStream, spanned: bool) -> TokenStream {
    let items = match parse_items(stream, spanned, &mut None) {
        Ok(items) => items,
        Err(error) => return error,
    };
    stream_expression(items, spanned)
}

/// `parsyng::ToTokens::to_tokens(&<interpolation>, &mut tokens);`
fn interpolate(interpolation: TokenTree) -> Item {
    let mut args = TokenStream::new();
    args.extend([
        punct('&'),
        interpolation,
        punct(','),
        punct('&'),
        ident("mut"),
        ident("tokens"),
    ]);
    let mut code = TokenStream::new();
    path(&mut code, &["ToTokens", "to_tokens"]);
    code.extend([group(Delimiter::Parenthesis, args), punct(';')]);
    Item::Code(code)
}

/// Registers `ident` as a variable iterated by the enclosing repetition:
/// `let ident = match ident.next() { Some(ident) => ident, None => break };`
fn register_repetition_variable(repetition: &mut Repetition, variable: &TokenTree) {
    let name = variable.to_string();
    if repetition.used.contains(&name) {
        return;
    }
    repetition.used.push(name);

    let mut match_body = TokenStream::new();
    match_body.extend([
        ident("Some"),
        group(Delimiter::Parenthesis, TokenStream::from(variable.clone())),
        joint('='),
        punct('>'),
        variable.clone(),
        punct(','),
        ident("None"),
        joint('='),
        punct('>'),
        ident("break"),
    ]);
    repetition.prologue.extend([
        ident("let"),
        variable.clone(),
        punct('='),
        ident("match"),
        variable.clone(),
        punct('.'),
        ident("next"),
        group(Delimiter::Parenthesis, TokenStream::new()),
        group(Delimiter::Brace, match_body),
        punct(';'),
    ]);
}

/// Translates `stream` into output items. `repetition` is the enclosing
/// `#(...)*`, if any, including through nested groups.
fn parse_items(
    stream: TokenStream,
    spanned: bool,
    repetition: &mut Option<Repetition>,
) -> Result<Vec<Item>, TokenStream> {
    let mut items = Vec::new();
    let mut iter = stream.into_iter().peekable();

    while let Some(tt) = iter.next() {
        let TokenTree::Punct(ref punct_token) = tt else {
            items.push(token_item(tt, spanned, repetition)?);
            continue;
        };
        if punct_token.as_char() != INTERPOLATION_CHAR {
            items.push(token_item(tt, spanned, repetition)?);
            continue;
        }
        match iter.peek() {
            // `#ident`
            Some(TokenTree::Ident(_)) => {
                let ident = iter.next().expect("peeked");
                if let Some(repetition) = repetition {
                    register_repetition_variable(repetition, &ident);
                }
                items.push(interpolate(ident));
            }
            // `#{ expression }`
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                let Some(TokenTree::Group(g)) = iter.next() else {
                    unreachable!()
                };
                items.push(interpolate(group(Delimiter::None, g.stream())));
            }
            // `#( ... ) <separator> *`
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                if repetition.is_some() {
                    return Err(make_compile_error(g.span(), {
                        let mut tk = TokenStream::new();
                        tk.extend([Literal::string(
                            "Quote repetition inside another repetition is forbidden",
                        )]);
                        tk
                    }));
                }
                let Some(TokenTree::Group(g)) = iter.next() else {
                    unreachable!()
                };

                // The separator: every token up to the `*`. Its last punct is
                // only joint because the `*` follows it.
                let mut separator_tokens = Vec::new();
                loop {
                    match iter.next() {
                        Some(TokenTree::Punct(p)) if p.as_char() == '*' => break,
                        Some(tt) => separator_tokens.push(tt),
                        None => break,
                    }
                }
                if let Some(TokenTree::Punct(p)) = separator_tokens.last_mut() {
                    let mut alone = Punct::new(p.as_char(), Spacing::Alone);
                    alone.set_span(p.span());
                    *p = alone;
                }
                let separator = separator_tokens
                    .into_iter()
                    .map(|tt| token_item(tt, spanned, &mut None))
                    .collect::<Result<Vec<_>, _>>()?;

                let mut inner = Some(Repetition {
                    prologue: TokenStream::new(),
                    used: Vec::new(),
                });
                let body = parse_items(g.stream(), spanned, &mut inner)?;
                let mut loop_body = inner.expect("still set").prologue;

                // `if !__quote_first { <separator> } __quote_first = false;`
                loop_body.extend([
                    ident("if"),
                    punct('!'),
                    ident("__quote_first"),
                    group(Delimiter::Brace, emit(separator, spanned)),
                    ident("__quote_first"),
                    punct('='),
                    ident("false"),
                    punct(';'),
                ]);
                loop_body.extend(emit(body, spanned));

                // `let mut __quote_first = true; loop { <body> }`
                let mut code = TokenStream::new();
                code.extend([
                    ident("let"),
                    ident("mut"),
                    ident("__quote_first"),
                    punct('='),
                    ident("true"),
                    punct(';'),
                    ident("loop"),
                    group(Delimiter::Brace, loop_body),
                ]);
                items.push(Item::Code(code));
            }
            _ => items.push(token_item(tt, spanned, repetition)?),
        }
    }
    Ok(items)
}

/// The item rebuilding a single non-interpolated `tt`. Groups recurse
/// through [`parse_items`]; a group without interpolations is encoded
/// statically with its contents, otherwise it is built from a block with its
/// own `tokens`.
fn token_item(
    tt: TokenTree,
    spanned: bool,
    repetition: &mut Option<Repetition>,
) -> Result<Item, TokenStream> {
    Ok(match tt {
        TokenTree::Group(g) => {
            let items = parse_items(g.stream(), spanned, repetition)?;
            if items.iter().all(|item| matches!(item, Item::Static(_))) {
                let mut ops = vec![match g.delimiter() {
                    Delimiter::Parenthesis => b'(',
                    Delimiter::Bracket => b'[',
                    Delimiter::Brace => b'{',
                    Delimiter::None => b'n',
                }];
                for item in items {
                    if let Item::Static(inner) = item {
                        ops.extend_from_slice(&inner);
                    }
                }
                ops.push(b')');
                return Ok(Item::Static(ops));
            }
            let mut args = TokenStream::new();
            path(
                &mut args,
                &["proc_macro", "Delimiter", delimiter_name(g.delimiter())],
            );
            args.extend([punct(',')]);
            args.extend(stream_expression(items, spanned));
            let helper = if spanned { "group_spanned" } else { "group" };
            Item::Tree(call_private(helper, with_span(args, spanned)))
        }
        TokenTree::Ident(i) => {
            let name = i.to_string();
            name.strip_prefix("r#")
                .map_or_else(|| text_op(b'i', &name), |raw| text_op(b'r', raw))
        }
        TokenTree::Punct(p) => {
            let tag = match p.spacing() {
                Spacing::Joint => b'j',
                Spacing::Alone => b'a',
            };
            // Rust punctuation characters are all ASCII.
            let ch = u8::try_from(p.as_char()).expect("ASCII punctuation");
            Item::Static(vec![tag, ch])
        }
        TokenTree::Literal(l) => text_op(b'l', &l.to_string()),
    })
}

/// A `<tag><len><text>` record (see `parsyng::quote::__private`).
fn text_op(tag: u8, text: &str) -> Item {
    let len = u16::try_from(text.len()).expect("quote!: token longer than 65535 bytes");
    let mut ops = Vec::with_capacity(3 + text.len());
    ops.push(tag);
    ops.extend_from_slice(&len.to_le_bytes());
    ops.extend_from_slice(text.as_bytes());
    Item::Static(ops)
}

const fn delimiter_name(delimiter: Delimiter) -> &'static str {
    match delimiter {
        Delimiter::Parenthesis => "Parenthesis",
        Delimiter::Brace => "Brace",
        Delimiter::Bracket => "Bracket",
        Delimiter::None => "None",
    }
}
