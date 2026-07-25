//! Associated type aliases, shared between trait items, impl items and
//! plain `type` items.

use crate::ToTokens;

use crate::{
    ast::{
        item::{GenericParams, TypeParamBounds, WhereClause, constant::ConstantItem},
        tokens::{self, Colon, Eq, Semicolon},
        r#type::Type,
        visibility::Visibility,
    },
    error::Diagnostics,
    parse::Parse,
    proc_macro::Ident,
};

/// A `type` item or associated type: `type Foo<T>: Bound where ... =
/// SomeType where ...;`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/type-aliases.html> and
/// <https://doc.rust-lang.org/reference/items/associated-items.html#associated-types>
#[derive(Clone, Debug)]
pub struct TypeAlias {
    type_token: tokens::Type,
    ident: Ident,
    generics_parameters: Option<GenericParams>,
    bounds: Option<(Colon, TypeParamBounds)>,
    where_clause: Option<WhereClause>,
    default: Option<(Eq, Type, Option<WhereClause>)>,
    semi: Semicolon,
}

/// A visible associated `type` or `const` item, as used where an associated
/// item may carry its own [`Visibility`] independently of the enclosing
/// block.
///
/// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html>
#[derive(Clone, Debug)]
pub enum AssociatedAlias {
    /// An associated `type`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-types>
    TypeAlias(Visibility, Box<TypeAlias>),
    /// An associated `const`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/associated-items.html#associated-constants>
    Const(Visibility, Box<ConstantItem>),
}

impl Parse for TypeAlias {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let type_token = input.parse()?;
        let ident = input.parse()?;
        let generics_parameters = input.try_parse().ok();
        let bounds = input.try_parse().ok();
        let where_clause = input.try_parse().ok();

        let default = if let Ok(eq) = input.peek_parse() {
            Some((eq, input.parse()?, input.try_parse().ok()))
        } else {
            None
        };
        Ok(Self {
            type_token,
            ident,
            generics_parameters,
            bounds,
            where_clause,
            default,
            semi: input.parse()?,
        })
    }
}

impl ToTokens for TypeAlias {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.type_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.generics_parameters.to_tokens(tokens);
        self.bounds.to_tokens(tokens);
        self.where_clause.to_tokens(tokens);
        self.default.to_tokens(tokens);
        self.semi.to_tokens(tokens);
    }
}

impl Parse for AssociatedAlias {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok((visibility, ty_alias)) = input.try_parse() {
            Ok(Self::TypeAlias(visibility, ty_alias))
        } else if let Ok((visibility, constant)) = input.try_parse() {
            Ok(Self::Const(visibility, constant))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected an item",
                input.span(),
            ))
        }
    }
}

impl ToTokens for AssociatedAlias {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::TypeAlias(visibility, type_alias) => {
                visibility.to_tokens(tokens);
                type_alias.to_tokens(tokens);
            }
            Self::Const(visibility, constant_item) => {
                visibility.to_tokens(tokens);
                constant_item.to_tokens(tokens);
            }
        }
    }
}
