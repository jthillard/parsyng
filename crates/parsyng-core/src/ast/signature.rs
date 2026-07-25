//! Function signatures, shared by free functions, trait methods, and
//! `extern` block declarations.

use crate::ToTokens;

use crate::{
    ast::{
        delimiter::Parenthesized,
        item::{GenericParams, Lifetime, WhereClause},
        pattern::Pattern,
        tokens::{
            And, Async, Colon, Comma, Const, DotDotDot, Extern, Fn, Mut, RArrow, SelfValue, Unsafe,
        },
        r#type::Type,
    },
    combinator::Punctuated,
    error::Diagnostics,
    parse::Parse,
    proc_macro::{Ident, Literal},
};

/// A function signature: `const async unsafe extern "C" fn name<T>(params)
/// -> T where ...`, without a body.
///
/// Used by [`FunctionItem`](crate::ast::item::function::FunctionItem) (with
/// a body attached) and directly by trait function declarations.
///
/// Reference: <https://doc.rust-lang.org/reference/items/functions.html>
#[derive(Clone, Debug)]
pub struct FnSignature {
    const_token: Option<Const>,
    async_token: Option<Async>,
    unsafe_token: Option<Unsafe>,
    extern_token: Option<(Extern, Option<Literal>)>,
    fn_token: Fn,
    ident: Ident,
    generics: Option<GenericParams>,
    params: Parenthesized<Punctuated<FnParam, Comma>>,
    return_type: Option<(RArrow, Type)>,
    where_clause: Option<WhereClause>,
}

/// One parameter in a [`FnSignature`]'s parameter list.
///
/// Reference: <https://doc.rust-lang.org/reference/items/functions.html#function-parameters>
#[derive(Clone, Debug)]
pub enum FnParam {
    /// A `self` receiver.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/functions.html#r-items.fn.params.self-pat>
    SelfParam(SelfParam),
    /// A typed parameter: `pattern: Type`.
    Typed(PatType),
    /// C-variadic parameter `...`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/functions.html#r-items.fn.params.varargs>
    Variadic(DotDotDot),
}

/// A `self` receiver parameter, e.g. `&'a mut self` or `self: Box<Self>`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/functions.html#r-items.fn.params.self-pat>
#[derive(Clone, Debug)]
pub struct SelfParam {
    reference: Option<(And, Option<Lifetime>)>,
    mutability: Option<Mut>,
    self_token: SelfValue,
    typed: Option<(Colon, Type)>,
}

/// A typed function parameter: `pattern: Type`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/functions.html#function-parameters>
#[derive(Clone, Debug)]
pub struct PatType {
    pat: Pattern,
    colon: Colon,
    ty: Type,
}

impl FnParam {
    /// This parameter's type, if it has one written out (`None` for
    /// [`Variadic`](Self::Variadic), and for a bare `self`/`&self` with no
    /// explicit `: Type` annotation).
    #[must_use]
    pub fn ty(&self) -> Option<&Type> {
        match self {
            Self::SelfParam(self_param) => self_param.typed.as_ref().map(|x| &x.1),
            Self::Typed(pat_type) => Some(&pat_type.ty),
            Self::Variadic(_) => None,
        }
    }
    /// This parameter's bound name.
    ///
    /// # Panics
    /// Panics (`todo!()`) if called on [`SelfParam`](Self::SelfParam) — not
    /// yet implemented.
    #[must_use]
    pub fn ident(&self) -> Option<&Ident> {
        match self {
            Self::SelfParam(_) => todo!(),
            Self::Typed(pat_type) => pat_type.pat.ident(),
            Self::Variadic(_) => None,
        }
    }
    /// Whether this parameter's pattern is `mut`.
    ///
    /// # Panics
    /// Panics (`todo!()`) if called on [`SelfParam`](Self::SelfParam) — not
    /// yet implemented.
    #[must_use]
    pub fn mutability(&self) -> Option<&Mut> {
        match self {
            Self::SelfParam(_) => todo!(),
            Self::Typed(pat_type) => pat_type.pat.mutability(),
            Self::Variadic(_) => None,
        }
    }
}

impl FnSignature {
    /// The function's name.
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        &self.ident
    }
    /// The `-> T` return type, or `None` if the function returns `()`.
    #[must_use]
    pub fn return_type(&self) -> Option<&Type> {
        self.return_type.as_ref().map(|x| &x.1)
    }
    /// The parenthesized, comma-separated parameter list.
    #[must_use]
    pub const fn args(&self) -> &Punctuated<FnParam, Comma> {
        self.params.inner_ref()
    }
}

impl Parse for FnSignature {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            const_token: input.try_parse().ok(),
            async_token: input.try_parse().ok(),
            unsafe_token: input.try_parse().ok(),
            extern_token: if let Ok(extern_token) = input.try_parse() {
                let abi = input.try_parse().ok();
                Some((extern_token, abi))
            } else {
                None
            },
            fn_token: input.parse()?,
            ident: input.parse()?,
            generics: input.try_parse().ok(),
            params: input.parse()?,
            return_type: input.try_parse().ok(),
            where_clause: input.try_parse().ok(),
        })
    }
}

impl Parse for FnParam {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(variadic) = input.try_parse() {
            Ok(Self::Variadic(variadic))
        } else if let Ok(self_param) = input.try_parse() {
            Ok(Self::SelfParam(self_param))
        } else if let Ok(pat) = input.try_parse() {
            Ok(Self::Typed(PatType {
                pat,
                colon: input.parse()?,
                ty: input.parse()?,
            }))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected a function parameter",
                input.span(),
            ))
        }
    }
}

impl Parse for SelfParam {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let mut reference = None;
        let mutability;
        if let Ok(and_token) = input.try_parse::<And>() {
            let lifetime = input.try_parse().ok();
            mutability = input.try_parse().ok();
            reference = Some((and_token, lifetime));
        } else {
            mutability = input.try_parse().ok();
        }
        let self_token: SelfValue = input.parse()?;
        let typed = if let Ok(colon) = input.peek_parse() {
            Some((colon, input.parse()?))
        } else {
            None
        };
        Ok(Self {
            reference,
            mutability,
            self_token,
            typed,
        })
    }
}

impl ToTokens for FnSignature {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.const_token.to_tokens(tokens);
        self.async_token.to_tokens(tokens);
        self.unsafe_token.to_tokens(tokens);
        self.extern_token.to_tokens(tokens);
        self.fn_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.generics.to_tokens(tokens);
        self.params.to_tokens(tokens);
        self.return_type.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
    }
}

impl ToTokens for FnParam {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::SelfParam(self_param) => self_param.to_tokens(tokens),
            Self::Typed(typed) => typed.to_tokens(tokens),
            Self::Variadic(variadic) => variadic.to_tokens(tokens),
        }
    }
}

impl ToTokens for SelfParam {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.reference.to_tokens(tokens);
        self.mutability.to_tokens(tokens);
        self.self_token.to_tokens(tokens);
        self.typed.to_tokens(tokens);
    }
}

impl ToTokens for PatType {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.pat.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.ty.to_tokens(tokens);
    }
}
