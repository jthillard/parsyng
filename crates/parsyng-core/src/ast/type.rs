//! Type expressions.

use crate::ToTokens;

use crate::ast::item::macro_item::MacroInvocationItem;
use crate::{
    ast::{
        delimiter::{Bracketed, Parenthesized},
        item::{Lifetime, TypeParamBounds},
        path::{TypePathSegment, parse_leading_path_sep, parse_path_tail},
        tokens::{
            And, As, Comma, Const, Dyn, Extern, Fn, Gt, Impl, Lt, Mut, Not, PathSep, RArrow,
            Semicolon, Star, Unsafe,
        },
    },
    combinator::Punctuated,
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::{Delimiter, Literal, Span, TokenStream},
};

/// A type expression: `T`, `&'a mut T`, `[T; N]`, `dyn Trait`, `fn(A) -> B`,
/// and so on.
///
/// Reference: <https://doc.rust-lang.org/reference/types.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum Type {
    /// A parenthesized type `(T)`, used for disambiguation (as opposed to a
    /// 1-element [`Tuple`](Self::Tuple) `(T,)`, which requires a trailing
    /// comma).
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/tuple.html>
    Paren(Box<Parenthesized<Self>>),
    /// `impl Trait`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/impl-trait.html>
    ImplTrait(TypeImplTrait),
    /// A path used as a type, e.g. `std::vec::Vec<T>`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-types>
    Path(Box<TypePath>),
    /// A tuple type: `(A, B)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/tuple.html>
    Tuple(Box<Parenthesized<Punctuated<Self, Comma>>>),
    /// The never type `!`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/never.html>
    Never(Not),
    /// A raw pointer type: `*const T` / `*mut T`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#raw-pointers-const-and-mut>
    Pointer(TypePointer),
    /// A reference type: `&'a mut T`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#references--and-mut>
    Reference(TypeReference),
    /// An array type: `[T; N]`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/array.html>
    Array(Box<Bracketed<TypeArray>>),
    /// A slice type: `[T]`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/slice.html>
    Slice(Box<Bracketed<Self>>),
    /// `dyn Trait`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/trait-object.html>
    DynTrait(TypeDynTrait),
    /// A fully qualified path: `<T as Trait>::Assoc`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#qualified-paths>
    QualifiedPath(Box<TypeQualifiedPath>),
    /// A bare function pointer type: `fn(A) -> B`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/types/function-pointer.html>
    BareFn(Box<TypeBareFn>),
    /// A macro invocation used as a type.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
    MacroInvocation(MacroInvocationItem),
}

impl Type {
    /// This type's span: the span of the whole group for a parenthesized,
    /// tuple, array or slice type, and the span of its first token
    /// otherwise.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Paren(paren) => paren.span(),
            Self::ImplTrait(impl_trait) => impl_trait.impl_token.span(),
            Self::Path(path) => path.span(),
            Self::Tuple(tuple) => tuple.span(),
            Self::Never(not) => not.span(),
            Self::Pointer(pointer) => pointer.star_token.span(),
            Self::Reference(reference) => reference.and_token.span(),
            Self::Array(array) => array.span(),
            Self::Slice(slice) => slice.span(),
            Self::DynTrait(dyn_trait) => dyn_trait.dyn_token.span(),
            Self::QualifiedPath(path) => path.lt_token.span(),
            Self::BareFn(bare_fn) => bare_fn.span(),
            Self::MacroInvocation(invocation) => invocation.span(),
        }
    }
}

/// A path used as a type, e.g. `std::vec::Vec<T>`. See
/// [`ast::path::SimplePath`](crate::ast::path::SimplePath) for the
/// generics-free equivalent used elsewhere (`use` trees, macro paths).
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-types>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypePath {
    start_token: Option<PathSep>,
    root: TypePathSegment,
    paths: Vec<(PathSep, TypePathSegment)>,
}

impl TypePath {
    /// Parse a path in expression position: every segment goes through
    /// [`TypePathSegment::parse_expression`] (turbofish-only generics).
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
    #[cfg(feature = "full")]
    pub(crate) fn parse_expression(input: &mut ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            start_token: input.try_parse::<PathSep>().ok(),
            root: TypePathSegment::parse_expression(input)?,
            paths: parse_expression_segments(input),
        })
    }

    /// The span of this path's first token (its leading `::`, if any).
    #[must_use]
    pub fn span(&self) -> Span {
        self.start_token
            .as_ref()
            .map_or_else(|| self.root.span(), |start| start.spans()[0])
    }
}

impl TypeBareFn {
    /// The span of this function pointer type's first token.
    #[must_use]
    pub fn span(&self) -> Span {
        if let Some(unsafety) = &self.unsafety {
            unsafety.span()
        } else if let Some((extern_token, _)) = &self.extern_token {
            extern_token.span()
        } else {
            self.fn_token.span()
        }
    }
}

/// A reference type: `&'a mut T`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#references--and-mut>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypeReference {
    and_token: And,
    lifetime: Option<Lifetime>,
    mutability: Option<Mut>,
    elem: Box<Type>,
}

/// Whether a [`TypePointer`] is `*const` or `*mut`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#raw-pointers-const-and-mut>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum TypePointerKind {
    /// `*const T`.
    Const(Const),
    /// `*mut T`.
    Mut(Mut),
}

/// A raw pointer type: `*const T` / `*mut T`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#raw-pointers-const-and-mut>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypePointer {
    star_token: Star,
    kind: TypePointerKind,
    elem: Box<Type>,
}

/// An array type: `[T; N]` (the length `N` is kept as a raw, unparsed
/// [`TokenStream`] rather than a full expression).
///
/// Reference: <https://doc.rust-lang.org/reference/types/array.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypeArray {
    elem: Box<Type>,
    semicolon: Semicolon,
    len: TokenStream,
}

/// An `impl Trait` type: `impl Trait + 'a`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/impl-trait.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypeImplTrait {
    impl_token: Impl,
    bounds: TypeParamBounds,
}

/// A `dyn Trait` type: `dyn Trait + 'a`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/trait-object.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypeDynTrait {
    dyn_token: Dyn,
    bounds: TypeParamBounds,
}

/// A bare function pointer type: `unsafe extern "C" fn(A, ...) -> B`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/function-pointer.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypeBareFn {
    unsafety: Option<Unsafe>,
    extern_token: Option<(Extern, Option<Literal>)>,
    fn_token: Fn,
    params: Parenthesized<Punctuated<BareFnParam, Comma>>,
    return_type: Option<(RArrow, Box<Type>)>,
}

/// One parameter of a [`TypeBareFn`]: a type, or the C-variadic `...`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/function-pointer.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub enum BareFnParam {
    /// A typed parameter.
    Type(Box<Type>),
    /// C-variadic parameter `...`.
    Variadic(crate::ast::tokens::DotDotDot),
}

/// A fully qualified path: `<T as Trait>::Assoc::Path`.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#qualified-paths>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct TypeQualifiedPath {
    lt_token: Lt,
    ty: Box<Type>,
    as_token: Option<(As, TypePath)>,
    gt_token: Gt,
    paths: Vec<(PathSep, TypePathSegment)>,
}

impl Parse for TypePath {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            start_token: parse_leading_path_sep(input),
            root: input.parse()?,
            paths: parse_path_tail(input, TypePathSegment::parse),
        })
    }
}

impl Parse for TypeReference {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            and_token: input.parse()?,
            lifetime: input.try_parse().ok(),
            mutability: input.try_parse().ok(),
            elem: Box::new(input.parse()?),
        })
    }
}

impl Parse for TypePointer {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let star_token = input.parse()?;
        let kind = if let Ok(const_token) = input.try_parse() {
            TypePointerKind::Const(const_token)
        } else if let Ok(mut_token) = input.try_parse() {
            TypePointerKind::Mut(mut_token)
        } else {
            return Err(Diagnostics::new_error_spanned(
                "Expected `const` or `mut` after `*`",
                input.span(),
            ));
        };
        Ok(Self {
            star_token,
            kind,
            elem: Box::new(input.parse()?),
        })
    }
}

impl Parse for TypeArray {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            elem: Box::new(input.parse()?),
            semicolon: input.parse()?,
            len: input.parse()?,
        })
    }
}

impl Parse for TypeImplTrait {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            impl_token: input.parse()?,
            bounds: input.parse()?,
        })
    }
}

impl Parse for TypeDynTrait {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            dyn_token: input.parse()?,
            bounds: input.parse()?,
        })
    }
}

impl Parse for BareFnParam {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(variadic) = input.try_parse() {
            Ok(Self::Variadic(variadic))
        } else {
            Ok(Self::Type(Box::new(input.parse()?)))
        }
    }
}

impl Parse for TypeBareFn {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let unsafety = input.try_parse().ok();
        let extern_token = if let Ok(extern_token) = input.try_parse() {
            let abi = input.try_parse().ok();
            Some((extern_token, abi))
        } else {
            None
        };
        let fn_token = input.parse()?;
        Ok(Self {
            unsafety,
            extern_token,
            fn_token,
            params: input.parse()?,
            return_type: input
                .try_parse::<(RArrow, Type)>()
                .ok()
                .map(|(arrow, ty)| (arrow, Box::new(ty))),
        })
    }
}

/// The `::segment` tail of an expression path, each segment parsed with
/// [`TypePathSegment::parse_expression`].
#[cfg(feature = "full")]
fn parse_expression_segments(input: &mut ParseBuffer) -> Vec<(PathSep, TypePathSegment)> {
    parse_path_tail(input, TypePathSegment::parse_expression)
}

impl TypeQualifiedPath {
    /// Parse a qualified path in expression position (`<T as Trait>::f`):
    /// the `<T as Trait>` header is parsed as for types, the trailing
    /// segments with turbofish-only generics.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#qualified-paths>
    #[cfg(feature = "full")]
    pub(crate) fn parse_expression(input: &mut ParseBuffer) -> crate::error::Result<Self> {
        Self::parse_with(input, TypePathSegment::parse_expression)
    }

    fn parse_with(
        input: &mut ParseBuffer,
        segment: fn(&mut ParseBuffer) -> crate::error::Result<TypePathSegment>,
    ) -> crate::error::Result<Self> {
        let lt_token = input.parse()?;
        let ty = Box::new(input.parse()?);
        let as_token = if let Ok(as_token) = input.try_parse() {
            Some((as_token, input.parse()?))
        } else {
            None
        };
        let gt_token = input.parse()?;
        let mut paths = Vec::new();
        let first_sep: PathSep = input.parse()?;
        paths.push((first_sep, segment(input)?));
        while let Ok(pair) =
            input.try_advance(|input| Ok((input.parse::<PathSep>()?, segment(input)?)))
        {
            paths.push(pair);
        }
        Ok(Self {
            lt_token,
            ty,
            as_token,
            gt_token,
            paths,
        })
    }
}

impl Parse for TypeQualifiedPath {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Self::parse_with(input, TypePathSegment::parse)
    }
}

impl Parse for Type {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        // Every alternative starts with a distinct token, so dispatch on it
        // instead of trying each one in turn.
        if let Some((ch, _)) = input.peek_punct_char() {
            match ch {
                '&' => return input.parse().map(Self::Reference),
                '*' => return input.parse().map(Self::Pointer),
                '!' => return input.parse::<Not>().map(Self::Never),
                '<' => return input.parse().map(Self::QualifiedPath),
                _ => {}
            }
        }
        match input.peek_ident_str() {
            Some("fn" | "unsafe" | "extern") => {
                return input.parse().map(|bare_fn| Self::BareFn(Box::new(bare_fn)));
            }
            Some("impl") => return input.parse().map(Self::ImplTrait),
            Some("dyn") => return input.parse().map(Self::DynTrait),
            _ => {}
        }
        if let Some(group) = input.peek_group() {
            match group.delimiter() {
                Delimiter::Parenthesis => {
                    let (group, mut inner) =
                        input.group_contents().expect("peeked group must exist");
                    let content: Punctuated<Self, Comma> = inner.parse()?;
                    if !inner.is_empty() {
                        return Err(Diagnostics::new_error_spanned(
                            "Unexpected tokens in tuple type",
                            inner.span(),
                        ));
                    }
                    if content.len() == 1 && content.trailing().is_some() {
                        let mut iter = content.into_iter();
                        let ty = iter.next().expect("len=1 guarantees element");
                        return Ok(Self::Paren(Box::new(Parenthesized::new(group, ty))));
                    }
                    return Ok(Self::Tuple(Box::new(Parenthesized::new(group, content))));
                }
                Delimiter::Bracket => {
                    if let Ok(array) = input.try_parse::<Bracketed<TypeArray>>() {
                        return Ok(Self::Array(Box::new(array)));
                    }
                    if let Ok(slice) = input.try_parse::<Bracketed<Self>>() {
                        return Ok(Self::Slice(Box::new(slice)));
                    }
                }
                _ => {}
            }
        }

        // A path, or a macro invocation if the path is followed by `!`.
        let mut fork = input.clone();
        let path = fork.parse()?;
        if fork.peek_punct_char().is_some_and(|(ch, _)| ch == '!') {
            return input.parse().map(Self::MacroInvocation);
        }
        *input = fork;
        Ok(Self::Path(path))
    }
}

impl ToTokens for TypePath {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.start_token.to_tokens(tokens);
        self.root.to_tokens(tokens);
        self.paths.to_tokens(tokens);
    }
}

impl ToTokens for TypeReference {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.and_token.to_tokens(tokens);
        self.lifetime.to_tokens(tokens);
        self.mutability.to_tokens(tokens);
        self.elem.to_tokens(tokens);
    }
}

impl ToTokens for TypePointer {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.star_token.to_tokens(tokens);
        self.kind.to_tokens(tokens);
        self.elem.to_tokens(tokens);
    }
}

impl ToTokens for TypePointerKind {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Const(const_token) => const_token.to_tokens(tokens),
            Self::Mut(mut_token) => mut_token.to_tokens(tokens),
        }
    }
}

impl ToTokens for TypeArray {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.elem.to_tokens(tokens);
        self.semicolon.to_tokens(tokens);
        self.len.to_tokens(tokens);
    }
}

impl ToTokens for TypeImplTrait {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.impl_token.to_tokens(tokens);
        self.bounds.to_tokens(tokens);
    }
}

impl ToTokens for TypeDynTrait {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.dyn_token.to_tokens(tokens);
        self.bounds.to_tokens(tokens);
    }
}

impl ToTokens for TypeBareFn {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.unsafety.to_tokens(tokens);
        self.extern_token.to_tokens(tokens);
        self.fn_token.to_tokens(tokens);
        self.params.to_tokens(tokens);
        self.return_type.to_tokens(tokens);
    }
}

impl ToTokens for BareFnParam {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Type(ty) => ty.to_tokens(tokens),
            Self::Variadic(variadic) => variadic.to_tokens(tokens),
        }
    }
}

impl ToTokens for TypeQualifiedPath {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.lt_token.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.as_token.to_tokens(tokens);
        self.gt_token.to_tokens(tokens);
        self.paths.to_tokens(tokens);
    }
}

impl ToTokens for Type {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Path(type_path) => type_path.to_tokens(tokens),
            Self::Reference(reference) => reference.to_tokens(tokens),
            Self::Pointer(pointer) => pointer.to_tokens(tokens),
            Self::Tuple(tuple) => tuple.to_tokens(tokens),
            Self::Paren(paren) => paren.to_tokens(tokens),
            Self::Array(array) => array.to_tokens(tokens),
            Self::Slice(slice) => slice.to_tokens(tokens),
            Self::ImplTrait(impl_trait) => impl_trait.to_tokens(tokens),
            Self::DynTrait(dyn_trait) => dyn_trait.to_tokens(tokens),
            Self::BareFn(bare_fn) => bare_fn.to_tokens(tokens),
            Self::QualifiedPath(qualified) => qualified.to_tokens(tokens),
            Self::Never(never) => never.to_tokens(tokens),
            Self::MacroInvocation(macro_invocation) => macro_invocation.to_tokens(tokens),
        }
    }
}
