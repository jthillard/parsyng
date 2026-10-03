//! A minimal token walker and output builder on top of the compiler's
//! `proc_macro`.
//!
//! This crate deliberately does not depend on `parsyng-core`, so that it
//! compiles in parallel with it: it only needs to recognise the outline of
//! a function signature or a type definition, and passes every other token
//! through untouched (keeping its span).

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// An error reported as a spanned `compile_error!`.
pub struct Error {
    span: Span,
    message: String,
}

impl Error {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    /// `compile_error! { "<message>" }`, spanned at the error.
    pub fn into_compile_error(self) -> TokenStream {
        let mut message = Literal::string(&self.message);
        message.set_span(self.span);
        let mut bang = Punct::new('!', Spacing::Alone);
        bang.set_span(self.span);
        let mut body = Group::new(Delimiter::Brace, TokenTree::from(message).into());
        body.set_span(self.span);
        [
            TokenTree::from(Ident::new("compile_error", self.span)),
            bang.into(),
            body.into(),
        ]
        .into_iter()
        .collect()
    }
}

pub type Result<T> = core::result::Result<T, Error>;

/// Accumulates output tokens.
#[derive(Default)]
pub struct Out(Vec<TokenTree>);

impl Out {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append Rust source text, spanned at the call site.
    pub fn src(&mut self, source: &str) -> &mut Self {
        let stream: TokenStream = source.parse().expect("generated code is valid Rust tokens");
        self.0.extend(stream);
        self
    }

    /// Append tokens as they are.
    pub fn tokens(&mut self, tokens: impl IntoIterator<Item = TokenTree>) -> &mut Self {
        self.0.extend(tokens);
        self
    }

    /// Append a single tree.
    pub fn tree(&mut self, tree: impl Into<TokenTree>) -> &mut Self {
        self.0.push(tree.into());
        self
    }

    /// Append a group delimited by `delimiter` around `inner`.
    pub fn group(&mut self, delimiter: Delimiter, inner: Self) -> &mut Self {
        self.0.push(Group::new(delimiter, inner.finish()).into());
        self
    }

    pub fn finish(self) -> TokenStream {
        self.0.into_iter().collect()
    }
}

/// A cursor over a flat list of token trees.
pub struct Cursor {
    tokens: Vec<TokenTree>,
    pos: usize,
}

impl Cursor {
    pub fn new(stream: TokenStream) -> Self {
        Self {
            tokens: stream.into_iter().collect(),
            pos: 0,
        }
    }

    pub fn peek(&self) -> Option<&TokenTree> {
        self.tokens.get(self.pos)
    }

    pub fn peek_nth(&self, n: usize) -> Option<&TokenTree> {
        self.tokens.get(self.pos + n)
    }

    pub const fn is_empty(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// The span of the next token, or the call site at the end.
    pub fn span(&self) -> Span {
        self.peek().map_or_else(Span::call_site, TokenTree::span)
    }

    pub fn next(&mut self) -> Option<TokenTree> {
        let tree = self.tokens.get(self.pos).cloned();
        if tree.is_some() {
            self.pos += 1;
        }
        tree
    }

    /// Whether the next token is the punctuation `ch`.
    pub fn peek_punct(&self, ch: char) -> bool {
        is_punct(self.peek(), ch)
    }

    /// Whether the next token is the identifier (or keyword) `name`.
    pub fn peek_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Some(TokenTree::Ident(ident)) if ident.to_string() == name)
    }

    /// Consume the identifier (or keyword) `name`, if it is next.
    pub fn eat_ident(&mut self, name: &str) -> bool {
        let found = self.peek_ident(name);
        if found {
            self.pos += 1;
        }
        found
    }

    /// Consume the punctuation `ch`, if it is next.
    pub fn eat_punct(&mut self, ch: char) -> bool {
        let found = self.peek_punct(ch);
        if found {
            self.pos += 1;
        }
        found
    }

    /// Consume an identifier.
    pub fn ident(&mut self, what: &str) -> Result<Ident> {
        match self.peek() {
            Some(TokenTree::Ident(ident)) => {
                let ident = ident.clone();
                self.pos += 1;
                Ok(ident)
            }
            _ => Err(Error::new(self.span(), format!("Expected {what}"))),
        }
    }

    /// Consume a group delimited by `delimiter`.
    pub fn group(&mut self, delimiter: Delimiter) -> Option<Group> {
        match self.peek() {
            Some(TokenTree::Group(group)) if group.delimiter() == delimiter => {
                let group = group.clone();
                self.pos += 1;
                Some(group)
            }
            _ => None,
        }
    }

    /// Consume every outer attribute (`#[...]`, doc comments included).
    pub fn attributes(&mut self) -> Vec<TokenTree> {
        let mut attributes = Vec::new();
        while self.peek_punct('#')
            && matches!(self.peek_nth(1), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket)
        {
            attributes.extend(self.next());
            attributes.extend(self.next());
        }
        attributes
    }

    /// Consume a visibility (`pub`, `pub(...)`), if any.
    pub fn visibility(&mut self) -> Vec<TokenTree> {
        let mut visibility = Vec::new();
        if self.peek_ident("pub") {
            visibility.extend(self.next());
            if let Some(group) = self.group(Delimiter::Parenthesis) {
                visibility.push(group.into());
            }
        }
        visibility
    }

    /// Consume tokens up to (not including) the first token for which
    /// `stop` returns `true` outside of any `<...>`.
    pub fn until(&mut self, mut stop: impl FnMut(&TokenTree) -> bool) -> Vec<TokenTree> {
        let mut depth = AngleDepth::default();
        let mut out = Vec::new();
        while let Some(tree) = self.peek() {
            if depth.is_top() && stop(tree) {
                break;
            }
            depth.track(tree, out.last());
            out.extend(self.next());
        }
        out
    }

    /// Every remaining token.
    pub fn rest(&mut self) -> Vec<TokenTree> {
        let rest = self.tokens.split_off(self.pos.min(self.tokens.len()));
        self.pos = self.tokens.len();
        rest
    }
}

pub fn is_punct(tree: Option<&TokenTree>, ch: char) -> bool {
    matches!(tree, Some(TokenTree::Punct(punct)) if punct.as_char() == ch)
}

/// Nesting depth of `<...>` in a type or generics list, ignoring the `>` of
/// `->` and `=>`.
#[derive(Default)]
pub struct AngleDepth(usize);

impl AngleDepth {
    pub const fn is_top(&self) -> bool {
        self.0 == 0
    }

    /// Account for `tree`, which follows `previous`.
    pub fn track(&mut self, tree: &TokenTree, previous: Option<&TokenTree>) {
        let TokenTree::Punct(punct) = tree else {
            return;
        };
        match punct.as_char() {
            '<' => self.0 += 1,
            '>' => {
                let arrow = matches!(previous, Some(TokenTree::Punct(p))
                    if p.spacing() == Spacing::Joint && matches!(p.as_char(), '-' | '='));
                if !arrow {
                    self.0 = self.0.saturating_sub(1);
                }
            }
            _ => {}
        }
    }
}

/// Split `tokens` at every top-level `,`, dropping empty pieces (a trailing
/// comma).
pub fn split_commas(tokens: impl IntoIterator<Item = TokenTree>) -> Vec<Vec<TokenTree>> {
    let mut pieces = Vec::new();
    let mut current: Vec<TokenTree> = Vec::new();
    let mut depth = AngleDepth::default();
    for tree in tokens {
        if depth.is_top() && is_punct(Some(&tree), ',') {
            pieces.push(core::mem::take(&mut current));
            continue;
        }
        depth.track(&tree, current.last());
        current.push(tree);
    }
    pieces.push(current);
    pieces.retain(|piece| !piece.is_empty());
    pieces
}

/// The generics of a type definition, split for an `impl` block.
#[derive(Default)]
#[allow(clippy::struct_field_names)]
pub struct Generics {
    /// `<params>` without defaults, or nothing.
    pub impl_generics: Vec<TokenTree>,
    /// `<names>`, or nothing.
    pub type_generics: Vec<TokenTree>,
    /// `where ...`, or nothing.
    pub where_clause: Vec<TokenTree>,
}

impl Generics {
    /// Parse `<...>` if it is next.
    pub fn parse_params(cursor: &mut Cursor) -> Self {
        let mut generics = Self::default();
        if !cursor.eat_punct('<') {
            return generics;
        }
        let mut depth = AngleDepth(1);
        let mut params = Vec::new();
        while let Some(tree) = cursor.next() {
            depth.track(&tree, params.last());
            if depth.is_top() {
                break;
            }
            params.push(tree);
        }

        let mut impl_params = Out::new();
        let mut type_params = Out::new();
        for param in split_commas(params) {
            // Drop the default: everything from the top-level `=`.
            let mut param_cursor = Cursor::new(param.into_iter().collect());
            let param = param_cursor.until(|tree| is_punct(Some(tree), '='));
            let mut param = Cursor::new(param.into_iter().collect());
            // Attributes stay on the `impl` parameter only.
            let attributes = param.attributes();
            let rest = param.rest();
            let name: Vec<TokenTree> = match rest.as_slice() {
                // `'a: 'b`
                [quote @ TokenTree::Punct(p), lifetime, ..] if p.as_char() == '\'' => {
                    vec![quote.clone(), lifetime.clone()]
                }
                // `const N: usize`
                [TokenTree::Ident(keyword), name, ..] if keyword.to_string() == "const" => {
                    vec![name.clone()]
                }
                // `T: Bound`
                [name, ..] => vec![name.clone()],
                [] => continue,
            };
            impl_params.tokens(attributes).tokens(rest).src(",");
            type_params.tokens(name).src(",");
        }
        generics.impl_generics = angled(impl_params);
        generics.type_generics = angled(type_params);
        generics
    }

    /// Parse `where ...` up to (not including) the body: a `{...}` group or
    /// a `;`.
    pub fn parse_where(&mut self, cursor: &mut Cursor) {
        if cursor.peek_ident("where") {
            self.where_clause = cursor.until(|tree| {
                matches!(tree, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace)
                    || is_punct(Some(tree), ';')
            });
        }
    }
}

/// `<inner>`
fn angled(inner: Out) -> Vec<TokenTree> {
    let mut out = Out::new();
    out.src("<").tokens(inner.0).src(">");
    out.0
}

/// The shape of a struct's or enum variant's fields.
pub enum Fields {
    /// `{ a: A, b: B }`, with the field names.
    Named(Vec<Ident>),
    /// `(A, B)`, with the field count.
    Unnamed(usize),
    /// No fields.
    Unit,
}

impl Fields {
    /// Parse a `{...}` or `(...)` field list if one is next.
    pub fn parse(cursor: &mut Cursor) -> Result<Self> {
        if let Some(group) = cursor.group(Delimiter::Brace) {
            let mut names = Vec::new();
            for field in split_commas(group.stream()) {
                let mut field = Cursor::new(field.into_iter().collect());
                field.attributes();
                field.visibility();
                names.push(field.ident("a field name")?);
            }
            Ok(Self::Named(names))
        } else if let Some(group) = cursor.group(Delimiter::Parenthesis) {
            Ok(Self::Unnamed(split_commas(group.stream()).len()))
        } else {
            Ok(Self::Unit)
        }
    }
}

/// The parts of a `#[derive]` input the derives need.
pub struct DeriveInput {
    pub ident: Ident,
    pub generics: Generics,
    pub data: Data,
}

pub enum Data {
    Struct(Fields),
    /// Each variant's name and fields.
    Enum(Vec<(Ident, Fields)>),
}

impl DeriveInput {
    pub fn parse(input: TokenStream) -> Result<Self> {
        let mut cursor = Cursor::new(input);
        cursor.attributes();
        cursor.visibility();
        let is_enum = if cursor.eat_ident("struct") {
            false
        } else if cursor.eat_ident("enum") {
            true
        } else {
            return Err(Error::new(cursor.span(), "Expected a struct or an enum"));
        };
        let ident = cursor.ident("a type name")?;
        let mut generics = Generics::parse_params(&mut cursor);
        generics.parse_where(&mut cursor);

        let data = if is_enum {
            let Some(body) = cursor.group(Delimiter::Brace) else {
                return Err(Error::new(cursor.span(), "Expected the enum's variants"));
            };
            let mut variants = Vec::new();
            // Variants only contain top-level commas between them (their
            // fields are groups), so no `<...>` tracking is needed here.
            let mut variant = Cursor::new(body.stream());
            while !variant.is_empty() {
                variant.attributes();
                let ident = variant.ident("a variant name")?;
                let fields = Fields::parse(&mut variant)?;
                // Skip a `= discriminant`.
                while variant.next().is_some_and(|tree| !is_punct(Some(&tree), ',')) {}
                variants.push((ident, fields));
            }
            Data::Enum(variants)
        } else {
            let fields = Fields::parse(&mut cursor)?;
            // A tuple struct's `where` clause comes after its fields.
            if generics.where_clause.is_empty() {
                generics.parse_where(&mut cursor);
            }
            Data::Struct(fields)
        };
        Ok(Self {
            ident,
            generics,
            data,
        })
    }
}
