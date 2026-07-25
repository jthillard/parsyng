//! Top-level items.
//!
//! Contains the [`Item`] enum, [`VisItem<T>`] (attributes + visibility +
//! inner item), [`DeriveInput`], and the
//! generics/where-clause/lifetime grammar shared by every item kind.
//!
//! Each concrete item kind (struct, enum, function, trait, impl, ...) lives
//! in its own submodule below and is re-exported here as `pub type ItemXxx =
//! VisItem<xxx::Xxx>` (e.g. [`ItemStruct`], [`ItemFunction`]).

use core::ops::Deref;
use std::ops::DerefMut;

use crate::{
    ToTokens,
    ast::generics::{ImplGenerics, TypeGenerics},
    combinator::{PunctuatedIter, PunctuatedIterMut},
    proc_macro::{Delimiter, Span, TokenStream},
};

use crate::{
    ast::{
        attributes::{Attribute, parse_outer_attributes},
        item::{
            associated::TypeAlias,
            constant::ConstantItem,
            enum_item::EnumItem,
            extern_block::ExternBlockItem,
            extern_crate::ExternCrateItem,
            function::FunctionItem,
            implementation::Implementation,
            macro_item::{MacroInvocationItem, MacroItem, MacroRulesItem},
            module::ModItem,
            static_item::StaticItem,
            r#struct::Struct,
            trait_item::TraitItem,
            r#use::UseItem,
        },
        tokens::{Colon, Comma, Const, Eq, For, Gt, Lt, Plus, Question, Quote, Where},
        r#type::{Type, TypePath},
        visibility::Visibility,
    },
    combinator::{Punctuated, StopOnError},
    error::Diagnostics,
    parse::{Parse, ParseBuffer},
    proc_macro::{Group, Ident},
};

pub mod associated;
pub mod constant;
pub mod enum_item;
pub mod extern_block;
pub mod extern_crate;
pub mod function;
pub mod impl_item;
pub mod implementation;
pub mod macro_item;
pub mod module;
pub mod static_item;
pub mod r#struct;
pub mod trait_item;
pub mod r#use;

/// A top-level Rust item: everything that can appear directly inside a
/// module or [`Crate`](crate::ast::crate_source::Crate) — a struct, enum,
/// function, trait, impl block, `use`, and so on.
///
/// Every variant already bundles the item's leading attributes and
/// visibility (via [`VisItem`]); parsing consumes those first, then tries
/// each concrete item kind in turn.
///
/// Reference: <https://doc.rust-lang.org/reference/items.html>
#[derive(Clone, Debug)]
pub enum Item {
    /// A `struct` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/structs.html>
    Struct(ItemStruct),
    /// A `const` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/constant-items.html>
    Const(ItemConst),
    /// A `type` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/type-aliases.html>
    TypeAlias(Box<ItemTypeAlias>),
    /// A `use` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/use-declarations.html>
    Use(ItemUse),
    /// An `extern crate` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/extern-crates.html>
    ExternCrate(ItemExternCrate),
    /// An `extern` block.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/external-blocks.html>
    ExternBlock(ItemExternBlock),
    /// A `mod` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/modules.html>
    Mod(ItemMod),
    /// An `enum` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/enumerations.html>
    Enum(ItemEnum),
    /// A free function item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/functions.html>
    Function(Box<ItemFunction>),
    /// A `trait` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/traits.html>
    Trait(ItemTrait),
    /// A `static` item.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/static-items.html>
    Static(ItemStatic),
    /// A `macro_rules!` declarative macro.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros-by-example.html>
    MacroRules(ItemMacroRules),
    /// A Rust-2.0-style `macro` declarative macro.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros-by-example.html>
    Macro(ItemMacro),
    /// A macro invocation in item position.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
    MacroInvocation(ItemMacroInvocation),
    /// An `impl` block.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/implementations.html>
    Impl(ItemImpl),
}

/// A `const` generic parameter, e.g. `const N: usize = 5` inside `<...>`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html#const-generics>
#[derive(Clone, Debug)]
pub struct ConstParam {
    const_token: Const,
    /// This parameter's name.
    pub ident: Ident,
    colon: Colon,
    ty: Type,
    default: Option<(Eq, Type)>,
}

/// Adds leading outer attributes and a [`Visibility`] to any inner item type
/// `T`.
///
/// This is the type every `ItemXxx` alias (e.g. [`ItemStruct`]) expands to;
/// it [`Deref`]s to `T`, so `T`'s own methods are callable directly on a
/// `VisItem<T>`.
#[derive(Clone, Debug)]
pub struct VisItem<T> {
    attributes: Vec<Attribute>,
    visibility: Visibility,
    item: T,
}

/// The input to a `#[derive(...)]` macro: either a struct or an enum (the
/// only two kinds `#[derive]` can be applied to).
///
/// This is the type a
/// function annotated with `#[parsyng::proc_macro_derive]` typically takes
/// as its input parameter.
///
/// Reference: <https://doc.rust-lang.org/reference/procedural-macros.html#derive-macros>
///
/// # Limitations
/// [`generics_parameters`](Self::generics_parameters),
/// [`generics_parameters_mut`](Self::generics_parameters_mut) and
/// [`split_generics_for_impl`](Self::split_generics_for_impl) only handle
/// the [`Struct`](Self::Struct) variant so far; calling them on
/// [`Enum`](Self::Enum) panics (`todo!()`).
#[derive(Clone, Debug)]
pub enum DeriveInput {
    /// Deriving on a `struct`.
    Struct(Box<ItemStruct>),
    /// Deriving on an `enum`.
    Enum(Box<ItemEnum>),
}

impl DeriveInput {
    /// This type's generic parameters, if any.
    ///
    /// # Panics
    /// Panics (`todo!()`) for the [`Enum`](Self::Enum) variant.
    #[must_use]
    pub fn generics_parameters(&self) -> Option<&GenericParams> {
        match self {
            Self::Struct(vis_item) => vis_item.generic_parameters(),
            Self::Enum(_vis_item) => todo!(),
        }
    }
    /// Mutable access to this type's generic parameters, for adding trait
    /// bounds before re-emitting them (see
    /// [`TypeParamBounds::push`](TypeParamBounds::push)).
    ///
    /// # Panics
    /// Panics (`todo!()`) for the [`Enum`](Self::Enum) variant.
    pub fn generics_parameters_mut(&mut self) -> Option<&mut GenericParams> {
        match self {
            Self::Struct(vis_item) => vis_item.generic_parameters_mut(),
            Self::Enum(_vis_item) => todo!(),
        }
    }
    /// Split this type's generics into the `impl<...>`, `Type<...>` and
    /// `where ...` pieces needed to build a trait impl.
    ///
    /// # Panics
    /// Panics (`todo!()`) for the [`Enum`](Self::Enum) variant.
    #[must_use]
    pub fn split_generics_for_impl(
        &self,
    ) -> (
        Option<ImplGenerics<'_>>,
        Option<TypeGenerics<'_>>,
        Option<&WhereClause>,
    ) {
        match self {
            Self::Struct(vis_item) => vis_item.split_generics_for_impl(),
            Self::Enum(_vis_item) => todo!(),
        }
    }
}

impl DeriveInput {
    /// The name of the struct or enum being derived on.
    #[must_use]
    pub fn ident(&self) -> &Ident {
        match self {
            Self::Struct(vis_item) => vis_item.ident(),
            Self::Enum(vis_item) => vis_item.ident(),
        }
    }
}

impl Parse for ConstParam {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            const_token: input.parse()?,
            ident: input.parse()?,
            colon: input.parse()?,
            ty: input.parse()?,
            default: input.try_parse().ok(),
        })
    }
}

impl<T> Deref for VisItem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}
impl<T> DerefMut for VisItem<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

impl ToTokens for ConstParam {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.const_token.to_tokens(tokens);
        self.ident.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.default.to_tokens(tokens);
    }
}

impl<T: Parse> Parse for VisItem<T> {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        Ok(Self {
            attributes,
            visibility: input.parse()?,
            item: input.parse()?,
        })
    }
}

impl<T: ToTokens> ToTokens for VisItem<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.attributes.to_tokens(tokens);
        self.visibility.to_tokens(tokens);
        self.item.to_tokens(tokens);
    }
}

impl Parse for Item {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        let visibility = input.parse()?;
        if let Ok(r#struct) = input.try_parse() {
            Ok(Self::Struct(VisItem {
                attributes,
                visibility,
                item: r#struct,
            }))
        } else if let Ok(const_item) = input.try_parse() {
            Ok(Self::Const(VisItem {
                attributes,
                visibility,
                item: const_item,
            }))
        } else if let Ok(type_alias) = input.try_parse() {
            Ok(Self::TypeAlias(Box::new(VisItem {
                attributes,
                visibility,
                item: type_alias,
            })))
        } else if let Ok(r#use) = input.try_parse() {
            Ok(Self::Use(VisItem {
                attributes,
                visibility,
                item: r#use,
            }))
        } else if let Ok(extern_crate) = input.try_parse() {
            Ok(Self::ExternCrate(VisItem {
                attributes,
                visibility,
                item: extern_crate,
            }))
        } else if let Ok(extern_block) = input.try_parse() {
            Ok(Self::ExternBlock(VisItem {
                attributes,
                visibility,
                item: extern_block,
            }))
        } else if let Ok(module) = input.try_parse() {
            Ok(Self::Mod(VisItem {
                attributes,
                visibility,
                item: module,
            }))
        } else if let Ok(enum_item) = input.try_parse() {
            Ok(Self::Enum(VisItem {
                attributes,
                visibility,
                item: enum_item,
            }))
        } else if let Ok(function_item) = input.try_parse() {
            Ok(Self::Function(Box::new(VisItem {
                attributes,
                visibility,
                item: function_item,
            })))
        } else if let Ok(trait_item) = input.try_parse() {
            Ok(Self::Trait(VisItem {
                attributes,
                visibility,
                item: trait_item,
            }))
        } else if let Ok(static_item) = input.try_parse() {
            Ok(Self::Static(VisItem {
                attributes,
                visibility,
                item: static_item,
            }))
        } else if let Ok(implementation) = input.try_parse() {
            Ok(Self::Impl(VisItem {
                attributes,
                visibility,
                item: implementation,
            }))
        } else if let Ok(macro_rules) = input.try_parse() {
            Ok(Self::MacroRules(VisItem {
                attributes,
                visibility,
                item: macro_rules,
            }))
        } else if let Ok(macro_item) = input.try_parse() {
            Ok(Self::Macro(VisItem {
                attributes,
                visibility,
                item: macro_item,
            }))
        } else if let Ok(macro_invocation) = input.try_parse() {
            Ok(Self::MacroInvocation(VisItem {
                attributes,
                visibility,
                item: macro_invocation,
            }))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected an item",
                input.span(),
            ))
        }
    }
}
impl Parse for DeriveInput {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let attributes = parse_outer_attributes(input);
        let visibility = input.parse()?;
        if let Ok(r#struct) = input.try_parse() {
            Ok(Self::Struct(Box::new(VisItem {
                attributes,
                visibility,
                item: r#struct,
            })))
        } else if let Ok(enum_item) = input.try_parse() {
            Ok(Self::Enum(Box::new(VisItem {
                attributes,
                visibility,
                item: enum_item,
            })))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected an derive input",
                input.span(),
            ))
        }
    }
}
impl ToTokens for Item {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Struct(vis_item) => vis_item.to_tokens(tokens),
            Self::Const(vis_item) => vis_item.to_tokens(tokens),
            Self::TypeAlias(vis_item) => vis_item.to_tokens(tokens),
            Self::Use(vis_item) => vis_item.to_tokens(tokens),
            Self::ExternCrate(vis_item) => vis_item.to_tokens(tokens),
            Self::ExternBlock(vis_item) => vis_item.to_tokens(tokens),
            Self::Mod(vis_item) => vis_item.to_tokens(tokens),
            Self::Enum(vis_item) => vis_item.to_tokens(tokens),
            Self::Function(vis_item) => vis_item.to_tokens(tokens),
            Self::Trait(vis_item) => vis_item.to_tokens(tokens),
            Self::Static(vis_item) => vis_item.to_tokens(tokens),
            Self::MacroRules(vis_item) => vis_item.to_tokens(tokens),
            Self::Macro(vis_item) => vis_item.to_tokens(tokens),
            Self::MacroInvocation(macro_invocation) => macro_invocation.to_tokens(tokens),
            Self::Impl(implementation) => implementation.to_tokens(tokens),
        }
    }
}
impl ToTokens for DeriveInput {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Struct(vis_item) => vis_item.to_tokens(tokens),
            Self::Enum(vis_item) => vis_item.to_tokens(tokens),
        }
    }
}

/// A [`Struct`] item with its attributes and visibility.
pub type ItemStruct = VisItem<Struct>;
/// A [`ConstantItem`] item with its attributes and visibility.
pub type ItemConst = VisItem<ConstantItem>;
/// A [`TypeAlias`] item with its attributes and visibility.
pub type ItemTypeAlias = VisItem<TypeAlias>;
/// A [`UseItem`] item with its attributes and visibility.
pub type ItemUse = VisItem<UseItem>;
/// An [`ExternCrateItem`] with its attributes and visibility.
pub type ItemExternCrate = VisItem<ExternCrateItem>;
/// An [`ExternBlockItem`] with its attributes and visibility.
pub type ItemExternBlock = VisItem<ExternBlockItem>;
/// A [`ModItem`] with its attributes and visibility.
pub type ItemMod = VisItem<ModItem>;
/// An [`EnumItem`] with its attributes and visibility.
pub type ItemEnum = VisItem<EnumItem>;
/// A [`FunctionItem`] with its attributes and visibility.
pub type ItemFunction = VisItem<FunctionItem>;
/// A [`TraitItem`] with its attributes and visibility.
pub type ItemTrait = VisItem<TraitItem>;
/// A [`StaticItem`] with its attributes and visibility.
pub type ItemStatic = VisItem<StaticItem>;
/// A [`MacroRulesItem`] with its attributes and visibility.
pub type ItemMacroRules = VisItem<MacroRulesItem>;
/// A [`MacroItem`] with its attributes and visibility.
pub type ItemMacro = VisItem<MacroItem>;
/// A [`MacroInvocationItem`] with its attributes and visibility.
pub type ItemMacroInvocation = VisItem<MacroInvocationItem>;
/// An [`Implementation`] with its attributes and visibility.
pub type ItemImpl = VisItem<Implementation>;

/// A `where` clause: `where T: Clone, 'a: 'b`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html#where-clauses>
#[derive(Clone, Debug)]
pub struct WhereClause {
    where_keyword: Where,
    generics: Punctuated<WhereClauseItem, Comma, StopOnError>,
}

/// One bound inside a [`WhereClause`]: either a lifetime bound (`'a: 'b`) or
/// a type bound (`T: Trait`).
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html#where-clauses>
#[derive(Clone, Debug)]
pub enum WhereClauseItem {
    /// A lifetime bound: `'a: 'b`.
    Lifetime(LifetimeWhereClauseItem),
    /// A type bound: `T: Trait`.
    Type(Box<TypeBoundWhereClauseItem>),
}

/// A lifetime bound inside a `where` clause: `'a: 'b + 'c`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html#where-clauses>
#[derive(Clone, Debug)]
pub struct LifetimeWhereClauseItem {
    lifetime: Lifetime,
    colon: Colon,
    lifetime_bounds: Punctuated<Lifetime, Plus, StopOnError>,
}

/// A type bound inside a `where` clause: `for<'a> T: Trait<'a>`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html#where-clauses>
#[derive(Clone, Debug)]
pub struct TypeBoundWhereClauseItem {
    for_lifetimes: Option<(For, GenericParams)>,
    ty: Type,
    colon: Colon,
    bounds: Option<TypeParamBounds>,
}

/// A generic parameter list: `<T: Clone, 'a, const N: usize>`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html>
#[derive(Clone, Debug)]
pub struct GenericParams {
    /// The opening `<`.
    pub start_token: Lt,
    /// The comma-separated list of parameters.
    pub generics: Punctuated<GenericParam, Comma, StopOnError>,
    /// The closing `>`.
    pub last_token: Gt,
}

impl GenericParams {
    /// Iterate over each parameter, in declaration order.
    #[must_use]
    pub fn iter(&self) -> PunctuatedIter<'_, GenericParam, Comma> {
        self.generics.iter()
    }
    /// Iterate mutably over each parameter, in declaration order.
    #[must_use]
    pub fn iter_mut(&mut self) -> PunctuatedIterMut<'_, GenericParam, Comma> {
        self.generics.iter_mut()
    }
}

impl<'a> IntoIterator for &'a GenericParams {
    type Item = &'a GenericParam;
    type IntoIter = PunctuatedIter<'a, GenericParam, Comma>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut GenericParams {
    type Item = &'a mut GenericParam;
    type IntoIter = PunctuatedIterMut<'a, GenericParam, Comma>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// One entry in a [`GenericParams`] list: a type, lifetime, or const
/// parameter.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html>
#[derive(Clone, Debug)]
pub enum GenericParam {
    /// A type parameter.
    Type(Box<TypeParam>),
    /// A lifetime parameter.
    Lifetime(LifetimeParam),
    /// A const parameter.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/items/generics.html#const-generics>
    Const(Box<ConstParam>),
}

/// A type generic parameter: `T: Bound1 + Bound2 = Default`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html>
#[derive(Clone, Debug)]
pub struct TypeParam {
    /// This parameter's name.
    pub ident: Ident,
    colon: Option<Colon>,
    /// This parameter's trait/lifetime bounds.
    pub bounds: TypeParamBounds,
    default: Option<(Eq, Type)>,
}

/// A `+`-separated list of trait/lifetime bounds: `Bound1 + Bound2 + 'a`.
///
/// Must not be empty (an empty list is represented by the absence of a
/// `TypeParamBounds` altogether, e.g. `TypeParam::bounds` being unbounded).
///
/// Reference: <https://doc.rust-lang.org/reference/trait-bounds.html>
#[derive(Clone, Debug)]
pub struct TypeParamBounds {
    bounds: Punctuated<TypeParamBound, Plus, StopOnError>,
}

impl Default for TypeParamBounds {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeParamBounds {
    /// An empty bound list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bounds: Punctuated::new(),
        }
    }
    /// Append one more bound, separated from the previous one by a `+`
    /// spanned at `bound`'s own span.
    pub fn push(&mut self, bound: TypeParamBound) {
        let separator = Plus::new(bound.span());
        self.bounds.push((bound, separator));
    }
}

/// One bound inside a [`TypeParamBounds`] list: a trait bound or a lifetime.
///
/// Reference: <https://doc.rust-lang.org/reference/trait-bounds.html>
#[derive(Clone, Debug)]
pub enum TypeParamBound {
    /// A trait bound.
    Trait(Box<TraitBound>),
    /// A lifetime bound.
    Lifetime(Lifetime),
}

impl TypeParamBound {
    /// This bound's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Trait(trait_bound) => trait_bound.span(),
            Self::Lifetime(lifetime) => lifetime.span(),
        }
    }
}

/// A lifetime generic parameter: `'a: 'b + 'c`.
///
/// Reference: <https://doc.rust-lang.org/reference/items/generics.html>
#[derive(Clone, Debug)]
pub struct LifetimeParam {
    lifetime: Lifetime,
    bounds: Option<(Colon, LifetimeBounds)>,
}

/// A trait bound, e.g. `?Sized`, `for<'a> Trait<'a>`, optionally wrapped in
/// parentheses (`(?Sized)`) — `group` records the parenthesizing
/// [`Group`], if present, so it can be re-emitted on the round trip.
///
/// Reference: <https://doc.rust-lang.org/reference/trait-bounds.html>
#[derive(Clone, Debug)]
pub struct TraitBound {
    group: Option<Group>,
    question: Option<Question>,
    for_lifetimes: Option<(For, GenericParams)>,
    path: TypePath,
}

impl TraitBound {
    /// This bound's span.
    #[must_use]
    pub fn span(&self) -> Span {
        self.path.span()
    }
}

/// A lifetime, e.g. `'a`.
///
/// Reference: <https://doc.rust-lang.org/reference/tokens.html#lifetimes-and-loop-labels>
#[derive(Clone, Debug)]
pub struct Lifetime {
    quote: Quote,
    ident: Ident,
}

impl Lifetime {
    /// This lifetime's span.
    #[must_use]
    pub fn span(&self) -> Span {
        self.quote.span()
    }
}

/// A `+`-separated, non-empty list of lifetime bounds: `'b + 'c`.
///
/// Reference: <https://doc.rust-lang.org/reference/trait-bounds.html#lifetime-bounds>
#[derive(Clone, Debug)]
pub struct LifetimeBounds {
    bounds: Punctuated<Lifetime, Plus, StopOnError>,
}

impl Parse for WhereClause {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            where_keyword: input.parse()?,
            generics: input.parse()?,
        })
    }
}

impl ToTokens for WhereClause {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.where_keyword.to_tokens(tokens);
        self.generics.to_tokens(tokens);
    }
}

impl Parse for WhereClauseItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(lifetime) = input.try_parse() {
            Ok(Self::Lifetime(lifetime))
        } else if let Ok(ty) = input.try_parse() {
            Ok(Self::Type(ty))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected a where clause item",
                input.span(),
            ))
        }
    }
}

impl ToTokens for WhereClauseItem {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Lifetime(lifetime_where_clause_item) => {
                lifetime_where_clause_item.to_tokens(tokens);
            }
            Self::Type(type_bound_where_clause_item) => {
                type_bound_where_clause_item.to_tokens(tokens);
            }
        }
    }
}

impl Parse for LifetimeWhereClauseItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            lifetime: input.parse()?,
            colon: input.parse()?,
            lifetime_bounds: input.parse()?,
        })
    }
}

impl ToTokens for LifetimeWhereClauseItem {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.lifetime.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.lifetime_bounds.to_tokens(tokens);
    }
}
impl Parse for TypeBoundWhereClauseItem {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            for_lifetimes: input.try_parse().ok(),
            ty: input.parse()?,
            colon: input.parse()?,
            bounds: input.try_parse().ok(),
        })
    }
}

impl ToTokens for TypeBoundWhereClauseItem {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.for_lifetimes.to_tokens(tokens);
        self.ty.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.bounds.to_tokens(tokens);
    }
}

impl Parse for Lifetime {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            quote: input.parse()?,
            ident: input.parse()?,
        })
    }
}

impl ToTokens for Lifetime {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.quote.to_tokens(tokens);
        self.ident.to_tokens(tokens);
    }
}

impl Parse for TypeParam {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let ident = input.parse()?;

        let (colon, bounds) = if let Ok(colon) = input.peek_parse() {
            (Some(colon), input.parse()?)
        } else {
            (None, TypeParamBounds::new())
        };

        let default = if let Ok(eq) = input.peek_parse() {
            Some((eq, input.parse()?))
        } else {
            None
        };

        Ok(Self {
            ident,
            colon,
            bounds,
            default,
        })
    }
}

impl Parse for LifetimeBounds {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let bounds: Punctuated<_, _, _> = input.parse()?;
        if bounds.is_empty() {
            Err(Diagnostics::new_error_spanned(
                "LifetimeBounds must not be empty !",
                input.span(),
            ))
        } else {
            Ok(Self { bounds })
        }
    }
}

impl Parse for TypeParamBounds {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let bounds: Punctuated<_, _, _> = input.parse()?;
        if bounds.is_empty() {
            Err(Diagnostics::new_error_spanned(
                "TypeParamBounds must not be empty !",
                input.span(),
            ))
        } else {
            Ok(Self { bounds })
        }
    }
}
impl ToTokens for TypeParamBounds {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.bounds.to_tokens(tokens);
    }
}
impl Parse for TypeParamBound {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(r#trait) = input.try_parse() {
            Ok(Self::Trait(r#trait))
        } else if let Ok(lifetime) = input.try_parse() {
            Ok(Self::Lifetime(lifetime))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected a type parameter bound",
                input.span(),
            ))
        }
    }
}
impl ToTokens for TypeParamBound {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Trait(trait_bound) => trait_bound.to_tokens(tokens),
            Self::Lifetime(lifetime) => lifetime.to_tokens(tokens),
        }
    }
}
impl Parse for TraitBound {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Parenthesis
        {
            let mut inner = ParseBuffer::new(group.stream());
            Ok(Self {
                group: input.group(),
                question: inner.peek_parse().ok(),
                for_lifetimes: inner.try_parse().ok(),
                path: inner.parse()?,
            })
        } else {
            Ok(Self {
                group: None,
                question: input.peek_parse().ok(),
                for_lifetimes: input.try_parse().ok(),
                path: input.parse()?,
            })
        }
    }
}

impl ToTokens for TraitBound {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if let Some(group) = &self.group {
            let mut inner_tokens = TokenStream::new();
            self.question.to_tokens(&mut inner_tokens);
            self.for_lifetimes.to_tokens(&mut inner_tokens);
            self.path.to_tokens(&mut inner_tokens);
            tokens.extend(Some(Group::new(group.delimiter(), inner_tokens)));
        } else {
            self.question.to_tokens(tokens);
            self.for_lifetimes.to_tokens(tokens);
            self.path.to_tokens(tokens);
        }
    }
}

impl ToTokens for TypeParam {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.ident.to_tokens(tokens);
        self.colon.to_tokens(tokens);
        self.bounds.to_tokens(tokens);
        self.default.to_tokens(tokens);
    }
}
impl ToTokens for LifetimeParam {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.lifetime.to_tokens(tokens);
        self.bounds.to_tokens(tokens);
    }
}
impl ToTokens for LifetimeBounds {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.bounds.to_tokens(tokens);
    }
}

impl ToTokens for GenericParam {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Type(ty) => ty.to_tokens(tokens),
            Self::Lifetime(lifetime_param) => lifetime_param.to_tokens(tokens),
            Self::Const(const_param) => const_param.to_tokens(tokens),
        }
    }
}

impl Parse for GenericParam {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(const_param) = input.try_parse() {
            Ok(Self::Const(const_param))
        } else if let Ok(lifetime_param) = input.try_parse() {
            Ok(Self::Lifetime(lifetime_param))
        } else if let Ok(ty) = input.try_parse() {
            Ok(Self::Type(ty))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected a generic parameter",
                input.span(),
            ))
        }
    }
}
impl Parse for LifetimeParam {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let lifetime = input.parse()?;

        let bounds = if let Ok(colon) = input.peek_parse() {
            Some((colon, input.parse()?))
        } else {
            None
        };
        Ok(Self { lifetime, bounds })
    }
}

impl ToTokens for GenericParams {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.start_token.to_tokens(tokens);
        self.generics.to_tokens(tokens);
        self.last_token.to_tokens(tokens);
    }
}
impl Parse for GenericParams {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            start_token: input.parse()?,
            generics: input.parse()?,
            last_token: input.parse()?,
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
        check::<TypePath>(quote! {
            FnOnce() -> T
        });
    }
}
