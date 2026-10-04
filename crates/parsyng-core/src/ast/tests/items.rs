use super::{check, fails, parse_prefix};
use crate as parsyng;
use crate::{
    ast::{
        crate_source::Crate,
        item::{
            Item,
            associated::{AssociatedAlias, TypeAlias},
            constant::ConstantItem,
            extern_block::ExternBlockItem,
            extern_crate::ExternCrateItem,
            function::FunctionItem,
            impl_item::ImplItem,
            implementation::Implementation,
            macro_item::{MacroInvocationItem, MacroItem, MacroRulesItem},
            module::ModItem,
            static_item::StaticItem,
            r#struct::{Struct, StructField},
            trait_item::{TraitItem, TraitItemMember},
            r#use::{UseItem, UseTree},
        },
    },
    proc_macro::TokenStream,
};
use parsyng_quote_macros::quote;

fn item_kind(tokens: TokenStream) -> &'static str {
    match check::<Item>(tokens) {
        Item::Struct(_) => "struct",
        Item::Union(_) => "union",
        Item::Const(_) => "const",
        Item::TypeAlias(_) => "type",
        Item::Use(_) => "use",
        Item::ExternCrate(_) => "extern crate",
        Item::ExternBlock(_) => "extern block",
        Item::Mod(_) => "mod",
        Item::Enum(_) => "enum",
        Item::Function(_) => "fn",
        Item::Trait(_) => "trait",
        Item::Static(_) => "static",
        Item::MacroRules(_) => "macro_rules",
        Item::Macro(_) => "macro",
        Item::MacroInvocation(_) => "macro invocation",
        Item::Impl(_) => "impl",
    }
}

#[test]
fn item_dispatches_on_every_kind() {
    let cases = [
        (quote! { #[a] pub struct S { x: u8 } }, "struct"),
        (quote! { pub(crate) const C: u8 = 1; }, "const"),
        (quote! { union U { a: u8, b: f32 } }, "union"),
        (quote! { type T<U> = Vec<U>; }, "type"),
        (quote! { pub use a::b; }, "use"),
        (quote! { extern crate alloc; }, "extern crate"),
        (quote! { extern "C" { fn f(); } }, "extern block"),
        (quote! { mod m; }, "mod"),
        (quote! { enum E { A } }, "enum"),
        (quote! { fn f() {} }, "fn"),
        (quote! { const fn f() {} }, "fn"),
        (quote! { async fn f() {} }, "fn"),
        (quote! { unsafe fn f() {} }, "fn"),
        (quote! { extern "C" fn f() {} }, "fn"),
        (quote! { trait T {} }, "trait"),
        (quote! { unsafe trait T {} }, "trait"),
        (quote! { static S: u8 = 0; }, "static"),
        (quote! { macro_rules! m { () => {} } }, "macro_rules"),
        (quote! { macro m { () => {} } }, "macro"),
        (quote! { m!(); }, "macro invocation"),
        (quote! { a::m! { x } }, "macro invocation"),
        (quote! { impl S {} }, "impl"),
        (quote! { unsafe impl Send for S {} }, "impl"),
    ];
    for (tokens, expected) in cases {
        let source = tokens.to_string();
        assert_eq!(item_kind(tokens), expected, "{source}");
    }
}

#[test]
fn functions() {
    check::<FunctionItem>(quote! { fn f() {} });
    check::<FunctionItem>(quote! { fn f(); });
    check::<FunctionItem>(quote! { const async unsafe extern "C" fn f() {} });
    check::<FunctionItem>(quote! { extern fn f() {} });
    check::<FunctionItem>(quote! {
        fn f<'a, T: Clone, const N: usize>(x: &'a T, (a, b): (u8, u8)) -> impl Iterator<Item = T> + 'a
        where
            T: 'a,
        {
            let _ = x;
            todo!()
        }
    });
    let function = check::<FunctionItem>(quote! { fn name(a: u8, mut b: u16) -> u32 { 0 } });
    let signature = function.signature();
    assert_eq!(signature.ident().to_string(), "name");
    assert_eq!(signature.args().len(), 2);
    assert!(signature.return_type().is_some());
}

#[test]
fn traits() {
    check::<TraitItem>(quote! { trait T {} });
    check::<TraitItem>(quote! { auto trait T {} });
    check::<TraitItem>(quote! { unsafe auto trait T {} });
    check::<TraitItem>(quote! { trait T<U>: Clone + Send + 'static where U: Copy {} });
    check::<TraitItem>(quote! {
        trait Iterator {
            /// The element type.
            type Item;
            type Assoc<'a>: Clone + 'a where Self: 'a;
            type Default = u8;
            const ID: u32;
            const DEFAULT: u32 = 1;
            fn next(&mut self) -> Option<Self::Item>;
            fn count(self) -> usize where Self: Sized { 0 }
            #[inline]
            async fn run(&self) {}
        }
    });

    for tokens in [
        quote! { type Item; },
        quote! { const C: u8; },
        quote! { fn f(&self); },
        quote! { #[a] fn f() {} },
    ] {
        check::<TraitItemMember>(tokens);
    }
    fails::<TraitItem>(quote! { trait {} });
    fails::<TraitItem>(quote! { trait T });
}

#[test]
fn implementations() {
    check::<Implementation>(quote! { impl S {} });
    check::<Implementation>(quote! { impl<T> S<T> where T: Clone {} });
    check::<Implementation>(quote! { impl<T: Clone> Trait<T> for S<T> {} });
    check::<Implementation>(quote! { unsafe impl<T> Send for S<T> {} });
    check::<Implementation>(quote! { impl !Send for S {} });
    check::<Implementation>(quote! { impl Trait for &mut S {} });
    check::<Implementation>(quote! { impl<T> Trait for [T] {} });
    check::<Implementation>(quote! { impl dyn Trait {} });
    check::<Implementation>(quote! { impl<const N: usize> Trait for [u8; N] {} });
    check::<Implementation>(quote! {
        impl Iterator for S {
            type Item = u8;
            const ID: u32 = 0;
            #[inline]
            pub fn next(&mut self) -> Option<u8> { None }
            pub(crate) const unsafe fn get(&self) -> u8 { 0 }
            m!();
            m! { x }
        }
    });

    for tokens in [
        quote! { type A = u8; },
        quote! { const A: u8 = 0; },
        quote! { #[inline] pub fn f() {} },
        quote! { m!(); },
    ] {
        check::<ImplItem>(tokens);
    }
}

#[test]
fn use_trees() {
    check::<UseItem>(quote! { use a; });
    check::<UseItem>(quote! { use a::b::c; });
    check::<UseItem>(quote! { use a::b as c; });
    check::<UseItem>(quote! { use a::b as _; });
    check::<UseItem>(quote! { use a::*; });
    check::<UseItem>(quote! { use a::{}; });
    check::<UseItem>(quote! { use a::{b, c as d, e::*, self, f::{g, h},}; });
    check::<UseItem>(quote! { use crate::a; });
    check::<UseItem>(quote! { use super::super::a; });
    check::<UseItem>(quote! { use ::std::fmt; });
    check::<UseItem>(quote! { use {a, b}; });
    check::<UseItem>(quote! { use r#type::r#struct; });

    assert!(matches!(check::<UseTree>(quote! { a }), UseTree::Name(_)));
    assert!(matches!(
        check::<UseTree>(quote! { a::b }),
        UseTree::Path(_)
    ));
    assert!(matches!(
        check::<UseTree>(quote! { a as b }),
        UseTree::Rename(_)
    ));
    assert!(matches!(check::<UseTree>(quote! { * }), UseTree::Glob(_)));
    assert!(matches!(
        check::<UseTree>(quote! { {a} }),
        UseTree::Group(_)
    ));

    fails::<UseItem>(quote! { use a });
    fails::<UseItem>(quote! { use a::; });
    fails::<UseItem>(quote! { use a as; });
}

#[test]
fn modules_statics_constants_externs() {
    check::<ModItem>(quote! { mod m; });
    check::<ModItem>(quote! { mod m {} });
    check::<ModItem>(quote! { mod m { #![allow(x)] fn f() {} } });
    fails::<ModItem>(quote! { mod m });

    check::<StaticItem>(quote! { static S: u8 = 0; });
    check::<StaticItem>(quote! { static mut S: &str = "s"; });
    check::<StaticItem>(quote! { static S: u8; });
    fails::<StaticItem>(quote! { static S = 0; });

    check::<ConstantItem>(quote! { const C: u8 = 1 + 2; });
    check::<ConstantItem>(quote! { const _: () = (); });
    check::<ConstantItem>(quote! { const C: [u8; 2] = [0; 2]; });
    fails::<ConstantItem>(quote! { const C = 1; });

    check::<ExternCrateItem>(quote! { extern crate std; });
    check::<ExternCrateItem>(quote! { extern crate std as _; });
    check::<ExternCrateItem>(quote! { extern crate self as name; });

    check::<ExternBlockItem>(quote! { extern {} });
    check::<ExternBlockItem>(quote! { extern "C" { fn f(x: i32) -> i32; static S: u8; } });
    check::<ExternBlockItem>(quote! { unsafe extern "C" { pub safe fn f(); } });
}

#[test]
fn type_aliases_and_associated() {
    check::<TypeAlias>(quote! { type A = u8; });
    check::<TypeAlias>(quote! { type A = m!(); });
    check::<TypeAlias>(quote! { type A = m! {}; });
    check::<TypeAlias>(quote! { type A<T> where T: Clone = Vec<T>; });
    check::<TypeAlias>(quote! { type A: Clone + Send; });
    check::<TypeAlias>(quote! { type A<'a>: 'a = &'a u8 where Self: 'a; });
    assert!(matches!(
        check::<AssociatedAlias>(quote! { pub type A = u8; }),
        AssociatedAlias::TypeAlias(_, _)
    ));
    assert!(matches!(
        check::<AssociatedAlias>(quote! { pub(crate) const A: u8 = 1; }),
        AssociatedAlias::Const(_, _)
    ));
}

#[test]
fn macros() {
    check::<MacroRulesItem>(quote! { macro_rules! m { ($x:expr) => { $x }; () => {} } });
    check::<MacroRulesItem>(quote! { macro_rules! m ( () => () ); });
    check::<MacroRulesItem>(quote! { macro_rules! m [ () => () ]; });
    check::<MacroItem>(quote! { macro m { () => {} } });

    check::<MacroInvocationItem>(quote! { m!(); });
    check::<MacroInvocationItem>(quote! { m![]; });
    check::<MacroInvocationItem>(quote! { m! {} });
    check::<MacroInvocationItem>(quote! { ::a::b::m!(x, y); });
    check::<MacroInvocationItem>(quote! { m! {}; });
    // In type position, the `;` belongs to the enclosing item.
    let (_, rest) = parse_prefix::<crate::ast::r#type::Type>(quote! { m!(); });
    assert_eq!(rest, ";");
    fails::<MacroInvocationItem>(quote! { m(); });
    fails::<MacroInvocationItem>(quote! { m!; });
}

#[test]
fn structs_with_full() {
    let field = check::<StructField>(quote! { #[a] pub(crate) r#type: Option<u8> });
    assert_eq!(field.ident().to_string(), "r#type");
    check::<Struct>(quote! { struct S<const N: usize = 3>([u8; N]); });
}

#[test]
fn crates() {
    check::<Crate>(quote! {});
    check::<Crate>(quote! {
        #![allow(dead_code)]
        #![doc = "Crate docs."]

        use std::fmt;

        /// A type.
        pub struct S;

        impl fmt::Display for S {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "S")
            }
        }

        mod inner {
            pub fn f() {}
        }

        macro_rules! m { () => {} }
        m!();
    });
    fails::<Crate>(quote! { struct S });
    fails::<Crate>(quote! { 1 + 1 });
}

#[test]
fn item_nodes() {
    let field = check::<StructField>(quote! { pub x: u8 });
    assert_eq!(field.ident().to_string(), "x");

    let struct_struct = check::<Struct>(quote! { struct Point<T> where T: Copy { pub x: T, } });
    assert_eq!(struct_struct.ident().to_string(), "Point");
    assert!(struct_struct.generic_parameters().is_some());

    let item_struct = check::<Struct>(quote! { struct Unit; });
    assert_eq!(item_struct.ident().to_string(), "Unit");

    check::<ConstantItem>(quote! { const VALUE: u8; });
    check::<TypeAlias>(quote! { type Assoc; });

    let aa_type = check::<AssociatedAlias>(quote! { type Assoc; });
    assert!(matches!(aa_type, AssociatedAlias::TypeAlias(_, _)));
    let aa_const = check::<AssociatedAlias>(quote! { const VALUE: u8; });
    assert!(matches!(aa_const, AssociatedAlias::Const(_, _)));

    check::<Implementation>(quote! { impl Type { type Assoc; const VALUE: u8; } });
    check::<Implementation>(quote! { impl !Trait for Type { type Assoc; } });

    let item_struct = check::<crate::ast::item::Item>(quote! { pub struct S; });
    assert!(matches!(item_struct, crate::ast::item::Item::Struct(_)));
    let item_impl = check::<crate::ast::item::Item>(quote! { impl Type { type Assoc; } });
    assert!(matches!(item_impl, crate::ast::item::Item::Impl(_)));
}

#[test]
fn bodies_are_parsed() {
    use crate::ast::item::function::FunctionBody;

    let function = check::<FunctionItem>(quote! {
        fn f() {
            #![allow(unused)]
            let x = 1;
            x + 1
        }
    });
    let FunctionBody::Block(block) = function.body() else {
        panic!("expected a body")
    };
    assert_eq!(block.inner_ref().inner_attributes().len(), 1);
    assert_eq!(block.inner_ref().statements().len(), 2);

    // Syntax errors inside bodies are now reported.
    fails::<FunctionItem>(quote! { fn f() { a b } });
    fails::<TraitItem>(quote! { trait T { fn f() { 1 + } } });
    fails::<ModItem>(quote! { mod m { struct } });
    fails::<ConstantItem>(quote! { const C: u8 = 1 +; });
    fails::<StaticItem>(quote! { static S: u8 = ; });
    fails::<ExternBlockItem>(quote! { extern "C" { fn f() {} } });

    check::<ModItem>(quote! { mod m { #![allow(x)] mod n { fn f() { g() } } } });
    check::<ModItem>(quote! { unsafe mod m {} });
    check::<ConstantItem>(quote! { const C: u8 = { let x = 1; x + 1 }; });
    check::<StaticItem>(quote! { static S: [u8; 2] = [0, 1]; });
    check::<TraitItem>(
        quote! { trait T { #![allow(x)] fn f(&self) -> u8 { self.g() + 1 } m!(); } },
    );
}

#[test]
fn extern_block_items() {
    use crate::ast::item::extern_block::ExternItemKind;

    let block = check::<ExternBlockItem>(quote! {
        unsafe extern "C" {
            #![allow(non_camel_case_types)]
            /// Docs.
            pub safe fn sqrt(x: f64) -> f64;
            pub unsafe fn free(p: *mut u8);
            fn printf(fmt: *const u8, ...) -> i32;
            pub safe static VERSION: u32;
            unsafe static mut ERRNO: i32;
            static PLAIN: u8;
            type Opaque;
            m!();
        }
    });
    let kinds: Vec<_> = block
        .items()
        .iter()
        .map(|item| match item.kind() {
            ExternItemKind::Function(..) => "fn",
            ExternItemKind::Static(..) => "static",
            ExternItemKind::Type(_) => "type",
            ExternItemKind::Macro(_) => "macro",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "fn", "fn", "fn", "static", "static", "static", "type", "macro"
        ]
    );
    assert_eq!(block.items()[0].attributes().len(), 1);
    fails::<ExternBlockItem>(quote! { extern "C" { const C: u8; } });
}

#[test]
fn impl_members_have_visibility() {
    use crate::ast::item::impl_item::ImplItemKind;

    type IsKind = fn(&ImplItemKind) -> bool;
    let cases: [(TokenStream, IsKind); 5] = [
        (quote! { pub const A: u8 = 1; }, |k| {
            matches!(k, ImplItemKind::Const(_))
        }),
        (quote! { pub(crate) type B = u8; }, |k| {
            matches!(k, ImplItemKind::Type(_))
        }),
        (quote! { #[inline] pub fn f() {} }, |k| {
            matches!(k, ImplItemKind::Function(_))
        }),
        (quote! { pub default fn f() {} }, |k| {
            matches!(k, ImplItemKind::Function(_))
        }),
        (quote! { m!(); }, |k| matches!(k, ImplItemKind::Macro(_))),
    ];
    for (tokens, is_expected) in cases {
        let source = tokens.to_string();
        let item = check::<ImplItem>(tokens);
        assert!(is_expected(item.kind()), "{source}");
    }
    let item = check::<ImplItem>(quote! { #[a] pub const A: u8 = 1; });
    assert!(matches!(
        item.visibility(),
        crate::ast::visibility::Visibility::Public(_)
    ));
    assert_eq!(item.attributes().len(), 1);
    // `default` is only a keyword before an item.
    check::<ImplItem>(quote! { fn default() -> Self { Self } });
    check::<ImplItem>(quote! { default!(); });
    check::<Implementation>(quote! { impl S { #![allow(x)] pub const A: u8 = 1; } });
}

#[test]
fn unions() {
    check::<Item>(quote! { union U { a: u8 } });
    check::<Item>(quote! { pub union U<'a, T: Copy> where T: 'a { a: &'a T, b: f32, } });
    // `union` is a weak keyword.
    check::<Item>(quote! { fn union() {} });
    check::<Item>(quote! { union!(); });
}

#[test]
fn nightly_const_syntax() {
    check::<Implementation>(quote! { impl const Default for S { fn default() -> Self { S } } });
    check::<Implementation>(quote! { impl<T: ~const Clone> const Clone for W<T> {} });
    check::<Implementation>(quote! { impl<T> const !Trait for T {} });
    check::<TraitItem>(quote! { const trait T {} });
    check::<TraitItem>(quote! { const unsafe trait T: [const] Super {} });
    check::<Item>(quote! { const trait T {} });
    check::<Item>(quote! { #[const_trait] pub trait T { fn f(&self); } });
    check::<FunctionItem>(quote! { const fn f<T: [const] Destruct>(x: T) {} });
    check::<FunctionItem>(quote! { const fn f() -> impl ~const Fn() { const || {} } });
    check::<FunctionItem>(quote! { fn f() { let c = const move |x: u8| x; } });
    check::<FunctionItem>(quote! { fn f() { match x { const { 1 + 1 } => {} _ => {} } } });
    check::<FunctionItem>(quote! { fn f<const N: usize>() where [(); N + 1]: {} });
    check::<FunctionItem>(quote! { fn f<const N: usize>() -> [u8; { N * 2 }] { [0; N * 2] } });
    check::<ConstantItem>(quote! { const C<T: Default>: usize = 1 where T: Clone; });
    check::<ConstantItem>(quote! { const C<'a, const N: usize>: &'a [u8; N] = &[0; N]; });
    check::<Struct>(quote! { struct S<const B: &'static str>; });
}
