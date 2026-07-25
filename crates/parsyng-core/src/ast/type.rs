//! Type expressions.

use crate::ToTokens;

use crate::ast::item::macro_item::MacroInvocationItem;
use crate::{
    ast::{
        delimiter::{Bracketed, Parenthesized},
        item::{Lifetime, TypeParamBounds},
        path::TypePathSegment,
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
#[derive(Clone, Debug)]
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
    /// This type's span.
    ///
    /// # Panics
    /// Only the [`Never`](Self::Never) variant is implemented so far; every
    /// other variant panics (`todo!()`).
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Never(not) => not.span(),
            _ => todo!(),
        }
    }
}

/// A path used as a type, e.g. `std::vec::Vec<T>`. See
/// [`ast::path::SimplePath`](crate::ast::path::SimplePath) for the
/// generics-free equivalent used elsewhere (`use` trees, macro paths).
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-types>
#[derive(Clone, Debug)]
pub struct TypePath {
    start_token: Option<PathSep>,
    root: TypePathSegment,
    paths: Vec<(PathSep, TypePathSegment)>,
}

impl TypePath {
    /// The span of this path's first segment.
    #[must_use]
    pub fn span(&self) -> Span {
        self.root.span()
    }
}

/// A reference type: `&'a mut T`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#references--and-mut>
#[derive(Clone, Debug)]
pub struct TypeReference {
    and_token: And,
    lifetime: Option<Lifetime>,
    mutability: Option<Mut>,
    elem: Box<Type>,
}

/// Whether a [`TypePointer`] is `*const` or `*mut`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#raw-pointers-const-and-mut>
#[derive(Clone, Debug)]
pub enum TypePointerKind {
    /// `*const T`.
    Const(Const),
    /// `*mut T`.
    Mut(Mut),
}

/// A raw pointer type: `*const T` / `*mut T`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/pointer.html#raw-pointers-const-and-mut>
#[derive(Clone, Debug)]
pub struct TypePointer {
    star_token: Star,
    kind: TypePointerKind,
    elem: Box<Type>,
}

/// An array type: `[T; N]` (the length `N` is kept as a raw, unparsed
/// [`TokenStream`] rather than a full expression).
///
/// Reference: <https://doc.rust-lang.org/reference/types/array.html>
#[derive(Clone, Debug)]
pub struct TypeArray {
    elem: Box<Type>,
    semicolon: Semicolon,
    len: TokenStream,
}

/// An `impl Trait` type: `impl Trait + 'a`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/impl-trait.html>
#[derive(Clone, Debug)]
pub struct TypeImplTrait {
    impl_token: Impl,
    bounds: TypeParamBounds,
}

/// A `dyn Trait` type: `dyn Trait + 'a`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/trait-object.html>
#[derive(Clone, Debug)]
pub struct TypeDynTrait {
    dyn_token: Dyn,
    bounds: TypeParamBounds,
}

/// A bare function pointer type: `unsafe extern "C" fn(A, ...) -> B`.
///
/// Reference: <https://doc.rust-lang.org/reference/types/function-pointer.html>
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub enum BareFnParam {
    /// A typed parameter.
    Type(Box<Type>),
    /// C-variadic parameter `...`.
    Variadic(crate::ast::tokens::DotDotDot),
}

/// A fully qualified path: `<T as Trait>::Assoc::Path`.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#qualified-paths>
#[derive(Clone, Debug)]
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
            start_token: input.try_parse::<PathSep>().ok(),
            root: input.parse()?,
            paths: input.parse()?,
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

impl Parse for TypeQualifiedPath {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
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
        paths.push((first_sep, input.parse()?));
        while let Ok(sep) = input.try_parse() {
            paths.push((sep, input.parse()?));
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

impl Parse for Type {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let mut diagnostics = Diagnostics::empty();

        if let Ok(reference) = input.try_parse() {
            return Ok(Self::Reference(reference));
        }
        if let Ok(pointer) = input.try_parse() {
            return Ok(Self::Pointer(pointer));
        }
        if let Ok(never) = input.try_parse::<Not>() {
            return Ok(Self::Never(never));
        }
        if let Ok(bare_fn) = input.try_parse() {
            return Ok(Self::BareFn(Box::new(bare_fn)));
        }
        if let Ok(impl_trait) = input.try_parse() {
            return Ok(Self::ImplTrait(impl_trait));
        }
        if let Ok(dyn_trait) = input.try_parse() {
            return Ok(Self::DynTrait(dyn_trait));
        }
        if let Ok(qualified) = input.try_parse() {
            return Ok(Self::QualifiedPath(qualified));
        }
        if let Ok(macro_invocation) = input.try_parse() {
            return Ok(Self::MacroInvocation(macro_invocation));
        }
        if let Some(group) = input.peek_group() {
            match group.delimiter() {
                Delimiter::Parenthesis => {
                    let group = input.group().expect("peeked group must exist");
                    let mut inner = ParseBuffer::new(group.stream());
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

        match input.try_parse() {
            Ok(ok) => return Ok(Self::Path(ok)),
            Err(err) => diagnostics.join(err),
        }
        Err(diagnostics)
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
