pub use crate::proc_macro;

pub fn push_lit_spanned(
    stream: proc_macro::TokenStream,
    span: proc_macro::Span,
    tokens: &mut proc_macro::TokenStream,
) {
    tokens.extend(stream.into_iter().map(|mut token| {
        token.set_span(span);
        token
    }));
}
pub fn push_group_spanned(
    delimiter: proc_macro::Delimiter,
    stream: proc_macro::TokenStream,
    span: proc_macro::Span,
    tokens: &mut proc_macro::TokenStream,
) {
    let mut group = proc_macro::Group::new(delimiter, stream);
    group.set_span(span);
    tokens.extend(core::iter::once(group));
}
pub fn push_ident_spanned(
    ident: &str,
    span: proc_macro::Span,
    tokens: &mut proc_macro::TokenStream,
) {
    tokens.extend(core::iter::once(proc_macro::Ident::new(ident, span)));
}
pub fn push_ident_raw_spanned(
    ident: &str,
    span: proc_macro::Span,
    tokens: &mut proc_macro::TokenStream,
) {
    tokens.extend(core::iter::once(proc_macro::Ident::new_raw(ident, span)));
}
pub fn push_punct_alone_spanned(
    punct: char,
    span: proc_macro::Span,
    tokens: &mut proc_macro::TokenStream,
) {
    let mut punct = proc_macro::Punct::new(punct, proc_macro::Spacing::Alone);
    punct.set_span(span);
    tokens.extend(core::iter::once(punct));
}
pub fn push_punct_joint_spanned(
    punct: char,
    span: proc_macro::Span,
    tokens: &mut proc_macro::TokenStream,
) {
    let mut punct = proc_macro::Punct::new(punct, proc_macro::Spacing::Joint);
    punct.set_span(span);
    tokens.extend(core::iter::once(punct));
}

pub fn push_lit(stream: proc_macro::TokenStream, tokens: &mut proc_macro::TokenStream) {
    tokens.extend(stream);
}
pub fn push_group(
    delimiter: proc_macro::Delimiter,
    stream: proc_macro::TokenStream,
    tokens: &mut proc_macro::TokenStream,
) {
    tokens.extend(core::iter::once(proc_macro::Group::new(delimiter, stream)));
}
pub fn push_ident(ident: &str, tokens: &mut proc_macro::TokenStream) {
    tokens.extend(core::iter::once(proc_macro::Ident::new(
        ident,
        proc_macro::Span::call_site(),
    )));
}
pub fn push_ident_raw(ident: &str, tokens: &mut proc_macro::TokenStream) {
    tokens.extend(core::iter::once(proc_macro::Ident::new_raw(
        ident,
        proc_macro::Span::call_site(),
    )));
}
pub fn push_punct_alone(punct: char, tokens: &mut proc_macro::TokenStream) {
    tokens.extend(core::iter::once(proc_macro::Punct::new(
        punct,
        proc_macro::Spacing::Alone,
    )));
}
pub fn push_punct_joint(punct: char, tokens: &mut proc_macro::TokenStream) {
    tokens.extend(core::iter::once(proc_macro::Punct::new(
        punct,
        proc_macro::Spacing::Joint,
    )));
}
