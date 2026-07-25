//! Paths ([`SimplePath`]) and the generics/Fn-trait-sugar grammar attached to
//! a type path segment ([`TypePathSegment`], [`GenericArgs`]).

use crate::ToTokens;
use crate::ast::delimiter::Parenthesized;
use crate::ast::tokens::{Eq, RArrow};

use crate::combinator::Either;
use crate::{
    ast::{
        item::Lifetime,
        tokens::{Comma, Gt, Lt, PathSep},
        r#type::Type,
    },
    combinator::{Punctuated, StopOnError},
    error::Diagnostics,
    parse::{Parse, Peekable},
    proc_macro::{Ident, Span},
};

/// A path with no generic arguments, e.g. `std::mem::swap` or
/// `::foo::bar`. Used where generics don't make sense syntactically — `use`
/// trees, macro invocation paths, `pub(in path::to::mod)`.
///
/// For a path that may carry generic arguments on its segments (`Vec<T>`,
/// `<T as Trait>::Assoc`), see
/// [`TypePath`](crate::ast::type::TypePath) instead.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#simple-paths>
#[derive(Clone, Debug)]
pub struct SimplePath {
    start_token: Option<PathSep>,
    root: Ident,
    paths: Vec<(PathSep, Ident)>,
}

impl Parse for SimplePath {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            start_token: input.try_parse::<PathSep>().ok(),
            root: input.parse()?,
            paths: input.parse()?,
        })
    }
}

impl SimplePath {
    /// This path's sole identifier, if it has no leading `::` and no
    /// additional `::`-separated segments.
    ///
    /// Used by [`ast::pattern`](crate::ast::pattern) to tell a bare
    /// identifier pattern (`name`) apart from a longer path pattern
    /// (`Foo::Bar`).
    #[must_use]
    pub(crate) const fn as_single_ident(&self) -> Option<&Ident> {
        if self.start_token.is_none() && self.paths.is_empty() {
            Some(&self.root)
        } else {
            None
        }
    }
}

impl ToTokens for SimplePath {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.start_token.to_tokens(tokens);
        self.root.to_tokens(tokens);
        self.paths.to_tokens(tokens);
    }
}

/// One segment of a [`TypePath`](crate::ast::type::TypePath), e.g. `Vec`
/// in `Vec<T>`, or `FnOnce` in `FnOnce(A) -> B`.
///
/// `args` covers both the angle-bracket form (`<T>`, via [`GenericArgs`]) and
/// the Fn-trait-sugar form (`(A) -> B`, via [`TypePathFn`]).
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone, Debug)]
pub struct TypePathSegment {
    path_ident: Ident,
    args: Option<(Option<PathSep>, Either<GenericArgs, TypePathFn>)>,
}

impl TypePathSegment {
    /// The span of this segment's identifier.
    #[must_use]
    pub fn span(&self) -> Span {
        self.path_ident.span()
    }
}

/// The `Fn`-trait sugar form of a path segment's arguments: `(A, B) -> C`.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone, Debug)]
pub struct TypePathFn {
    inputs: Parenthesized<Option<TypePathFnInputs>>,
    return_type: Option<(RArrow, Box<Type>)>,
}

/// The comma-separated argument types inside [`TypePathFn`]'s parentheses.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone, Debug)]
pub struct TypePathFnInputs {
    args: Punctuated<Comma, Type>,
    trailing_comma: Option<Comma>,
}

/// Angle-bracketed generic arguments on a path segment: `<A, 'a, B = C>`.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone, Debug)]
pub struct GenericArgs {
    start_token: Lt,
    generics: Punctuated<GenericArg, Comma, StopOnError>,
    last_token: Gt,
}

/// One argument inside [`GenericArgs`]: a type, a lifetime, or an associated
/// type binding.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
#[derive(Clone, Debug)]
pub enum GenericArg {
    /// A type argument.
    Type(Box<Type>),
    /// A lifetime argument.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/tokens.html#lifetimes-and-loop-labels>
    Lifetime(Lifetime),
    /// An associated-type binding, e.g. `Item = T` in `Iterator<Item = T>`
    /// (the optional nested [`GenericArgs`] covers a binding on a generic
    /// associated type, e.g. `Item<'a> = T`).
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-types>
    Bindings(Ident, Option<Box<GenericArgs>>, Eq, Box<Type>),
}

impl ToTokens for TypePathSegment {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path_ident.to_tokens(tokens);
        self.args.to_tokens(tokens);
    }
}

impl Parse for TypePathSegment {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            path_ident: input.parse()?,
            args: input
                .try_parse::<(Option<Peekable<_>>, _)>()
                .ok()
                .map(|(sep, generics)| (sep.map(Peekable::inner), generics)),
        })
    }
}
impl ToTokens for GenericArg {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Type(ty) => ty.to_tokens(tokens),
            Self::Lifetime(lifetime) => lifetime.to_tokens(tokens),
            Self::Bindings(ident, generics, eq, ty) => (ident, generics, eq, ty).to_tokens(tokens),
        }
    }
}

impl Parse for GenericArg {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok((ident, generics, eq, ty)) = input.try_parse::<(_, Option<Peekable<_>>, _, _)>() {
            Ok(Self::Bindings(ident, generics.map(Peekable::inner), eq, ty))
        } else if let Ok(ty) = input.try_parse() {
            Ok(Self::Type(Box::new(ty)))
        } else if let Ok(lifetime) = input.try_parse() {
            Ok(Self::Lifetime(lifetime))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected a generic argument",
                input.span(),
            ))
        }
    }
}
impl ToTokens for GenericArgs {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.start_token.to_tokens(tokens);
        self.generics.to_tokens(tokens);
        self.last_token.to_tokens(tokens);
    }
}
impl Parse for GenericArgs {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            start_token: input.parse()?,
            generics: input.parse()?,
            last_token: input.parse()?,
        })
    }
}
impl ToTokens for TypePathFn {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.inputs.to_tokens(tokens);
        self.return_type.to_tokens(tokens);
    }
}

impl Parse for TypePathFn {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            inputs: input
                .parse::<Parenthesized<Option<Peekable<_>>>>()?
                .map(|inputs| inputs.map(Peekable::inner)),
            return_type: input.try_parse().ok(),
        })
    }
}
impl ToTokens for TypePathFnInputs {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.args.to_tokens(tokens);
        self.trailing_comma.to_tokens(tokens);
    }
}

impl Parse for TypePathFnInputs {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            args: input.parse()?,
            trailing_comma: input.parse()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate as parsyng;
    use parsyng_quote_macros::quote;

    use super::*;
    use crate::ast::tests::check;

    #[test]
    fn test_type_path() {
        check::<TypePathSegment>(quote! {
            Iterator<Item = &Attribute>
        });
    }
}
