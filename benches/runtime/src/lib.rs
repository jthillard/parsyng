//! Shared fixtures for the runtime benchmarks.
//!
//! The quote templates are written once as `macro_rules!` callbacks taking the
//! quasi-quoting macro to invoke and the interpolation syntax of the ident
//! (`(#ident)` for quote/parsyng/unsynn, `({{ ident }})` for moxy), so every
//! library expands exactly the same tokens.

/// Small `impl` block, the same as `bench-quote-comptime`'s `small` case.
#[macro_export]
macro_rules! small_template {
    ($($m:ident)::+, ($($id:tt)+)) => {
        $($m)::+! {
            impl $($id)+ {
                pub fn bar<T: Foo>(&mut self) -> Self {
                    // Some code
                    if true {
                        Self
                    } else {
                        unreachable!()
                    }
                }
            }
        }
    };
}

/// A serde-generated `Deserialize` impl (~180 lines).
#[macro_export]
macro_rules! big_template {
    ($($m:ident)::+, ($($id:tt)+)) => {
        $($m)::+! {
            impl<'de> _serde::Deserialize<'de> for $($id)+ {
                fn deserialize<__D>(__deserializer: __D) -> _serde::export::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    #[allow(non_camel_case_types)]
                    enum __Field {
                        __field0,
                        __field1,
                        __ignore,
                    }
                    struct __FieldVisitor;
                    impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                        type Value = __Field;
                        fn expecting(
                            &self,
                            __formatter: &mut _serde::export::Formatter,
                        ) -> _serde::export::fmt::Result {
                            _serde::export::Formatter::write_str(__formatter, "field identifier")
                        }
                        fn visit_u64<__E>(self, __value: u64) -> _serde::export::Result<Self::Value, __E>
                        where
                            __E: _serde::de::Error,
                        {
                            match __value {
                                0u64 => _serde::export::Ok(__Field::__field0),
                                1u64 => _serde::export::Ok(__Field::__field1),
                                _ => _serde::export::Err(_serde::de::Error::invalid_value(
                                    _serde::de::Unexpected::Unsigned(__value),
                                    &"field index 0 <= i < 2",
                                )),
                            }
                        }
                        fn visit_str<__E>(self, __value: &str) -> _serde::export::Result<Self::Value, __E>
                        where
                            __E: _serde::de::Error,
                        {
                            match __value {
                                "id" => _serde::export::Ok(__Field::__field0),
                                "s" => _serde::export::Ok(__Field::__field1),
                                _ => _serde::export::Ok(__Field::__ignore),
                            }
                        }
                        fn visit_bytes<__E>(
                            self,
                            __value: &[u8],
                        ) -> _serde::export::Result<Self::Value, __E>
                        where
                            __E: _serde::de::Error,
                        {
                            match __value {
                                b"id" => _serde::export::Ok(__Field::__field0),
                                b"s" => _serde::export::Ok(__Field::__field1),
                                _ => _serde::export::Ok(__Field::__ignore),
                            }
                        }
                    }
                    impl<'de> _serde::Deserialize<'de> for __Field {
                        #[inline]
                        fn deserialize<__D>(__deserializer: __D) -> _serde::export::Result<Self, __D::Error>
                        where
                            __D: _serde::Deserializer<'de>,
                        {
                            _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                        }
                    }
                    struct __Visitor<'de> {
                        marker: _serde::export::PhantomData<$($id)+>,
                        lifetime: _serde::export::PhantomData<&'de ()>,
                    }
                    impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                        type Value = $($id)+;
                        fn expecting(
                            &self,
                            __formatter: &mut _serde::export::Formatter,
                        ) -> _serde::export::fmt::Result {
                            _serde::export::Formatter::write_str(__formatter, "struct")
                        }
                        #[inline]
                        fn visit_seq<__A>(
                            self,
                            mut __seq: __A,
                        ) -> _serde::export::Result<Self::Value, __A::Error>
                        where
                            __A: _serde::de::SeqAccess<'de>,
                        {
                            let __field0 =
                                match try!(_serde::de::SeqAccess::next_element::<u64>(&mut __seq)) {
                                    _serde::export::Some(__value) => __value,
                                    _serde::export::None => {
                                        return _serde::export::Err(_serde::de::Error::invalid_length(
                                            0usize,
                                            &"struct with 2 elements",
                                        ));
                                    }
                                };
                            let __field1 =
                                match try!(_serde::de::SeqAccess::next_element::<String>(&mut __seq)) {
                                    _serde::export::Some(__value) => __value,
                                    _serde::export::None => {
                                        return _serde::export::Err(_serde::de::Error::invalid_length(
                                            1usize,
                                            &"struct with 2 elements",
                                        ));
                                    }
                                };
                            _serde::export::Ok($($id)+ {
                                id: __field0,
                                s: __field1,
                            })
                        }
                        #[inline]
                        fn visit_map<__A>(
                            self,
                            mut __map: __A,
                        ) -> _serde::export::Result<Self::Value, __A::Error>
                        where
                            __A: _serde::de::MapAccess<'de>,
                        {
                            let mut __field0: _serde::export::Option<u64> = _serde::export::None;
                            let mut __field1: _serde::export::Option<String> = _serde::export::None;
                            while let _serde::export::Some(__key) =
                                try!(_serde::de::MapAccess::next_key::<__Field>(&mut __map))
                            {
                                match __key {
                                    __Field::__field0 => {
                                        if _serde::export::Option::is_some(&__field0) {
                                            return _serde::export::Err(
                                                <__A::Error as _serde::de::Error>::duplicate_field("id"),
                                            );
                                        }
                                        __field0 = _serde::export::Some(
                                            try!(_serde::de::MapAccess::next_value::<u64>(&mut __map)),
                                        );
                                    }
                                    __Field::__field1 => {
                                        if _serde::export::Option::is_some(&__field1) {
                                            return _serde::export::Err(
                                                <__A::Error as _serde::de::Error>::duplicate_field("s"),
                                            );
                                        }
                                        __field1 = _serde::export::Some(
                                            try!(_serde::de::MapAccess::next_value::<String>(&mut __map)),
                                        );
                                    }
                                    _ => {
                                        let _ = try!(_serde::de::MapAccess::next_value::<
                                            _serde::de::IgnoredAny,
                                        >(&mut __map));
                                    }
                                }
                            }
                            let __field0 = match __field0 {
                                _serde::export::Some(__field0) => __field0,
                                _serde::export::None => try!(_serde::private::de::missing_field("id")),
                            };
                            let __field1 = match __field1 {
                                _serde::export::Some(__field1) => __field1,
                                _serde::export::None => try!(_serde::private::de::missing_field("s")),
                            };
                            _serde::export::Ok($($id)+ {
                                id: __field0,
                                s: __field1,
                            })
                        }
                    }
                    const FIELDS: &'static [&'static str] = &["id", "s"];
                    _serde::Deserializer::deserialize_struct(
                        __deserializer,
                        stringify!($($id)+),
                        FIELDS,
                        __Visitor {
                            marker: _serde::export::PhantomData::<$($id)+>,
                            lifetime: _serde::export::PhantomData,
                        },
                    )
                }
            }
        }
    };
}

/// A `DeriveInput`-shaped struct: attributes, lifetime/type/const generics, a
/// where-clause and a mix of field types.
pub const DERIVE_INPUT: &str = r#"
#[derive(Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config<'a, T: Clone + Default, const N: usize>
where
    T: Send + 'static,
{
    /// The name.
    pub name: &'a str,
    pub(crate) values: Vec<T>,
    #[serde(default)]
    pub enabled: bool,
    count: [u8; N],
    map: std::collections::HashMap<String, Option<Box<dyn Fn() -> T + Send>>>,
    pub cb: fn(&mut T, usize) -> Result<(), String>,
    inner: Option<Vec<(u32, &'a [T])>>,
    pub id: u64,
    ratio: f64,
    marker: core::marker::PhantomData<*const T>,
}
"#;

/// Real-world source files (from tokio) used for whole-file parsing, shared
/// with `parsyng-core`'s round-trip tests.
pub const FILES: &[(&str, &str)] = &[
    ("broadcast", include_str!("../../../crates/parsyng-core/tests/test_files/broadcast.rs")),
    ("delay_queue", include_str!("../../../crates/parsyng-core/tests/test_files/delay_queue.rs")),
    ("entry", include_str!("../../../crates/parsyng-core/tests/test_files/entry.rs")),
    ("local", include_str!("../../../crates/parsyng-core/tests/test_files/local.rs")),
];

/// The top-level items of `src` that moxy parses *faithfully* (re-emitting
/// the same tokens), re-emitted as source.
///
/// moxy 0.5 rejects `crate::`/`super::`/`self::` paths, so whole real-world
/// files never parse, and it silently drops some statements (e.g. `while`
/// loops in some positions). This subset lets all full Rust parsers do the
/// same work on the same input.
///
/// # Panics
/// Panics if `src` is not valid Rust according to syn.
#[must_use]
pub fn common_subset(src: &str) -> String {
    use quote::ToTokens as _;

    syn::parse_file(src)
        .unwrap()
        .items
        .iter()
        .map(|item| item.to_token_stream().to_string())
        .filter(|item| {
            moxy::parse!(item as moxy::ast::Item).is_ok_and(|parsed| {
                // moxy loses joint spacing in macro bodies; ignore whitespace.
                let strip = |s: &str| s.split_whitespace().collect::<String>();
                strip(&moxy::token::ToTokenStream::to_token_stream(&parsed).to_string()) == strip(item)
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Field names used by the `repetition` quote case.
#[must_use]
pub fn field_names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("field_{i}")).collect()
}

/// A minimal hand-written grammar for [`DERIVE_INPUT`]-like structs.
///
/// unsynn ships no Rust grammar, so this only recognises what a derive needs
/// (attributes, visibility, name, opaque generics/where-clause, named fields
/// with opaque types). It is *not* equivalent to a full `DeriveInput` parser.
pub mod unsynn_grammar {
    #![allow(missing_docs, clippy::result_large_err)]
    use unsynn::{
        BraceGroup, BraceGroupContaining, BracketGroup, Colon, Comma, CommaDelimitedVec, Cons,
        Either, Except, Ident, ParenthesisGroup, Pound, PunctAny, TokenTree, keyword, unsynn,
    };

    keyword! {
        pub KwStruct = "struct";
        pub KwPub = "pub";
        pub KwWhere = "where";
    }

    /// `<` / `>` regardless of spacing (`>>`, `<'a` are lexed as joint).
    pub type AnyLt = PunctAny<'<'>;
    pub type AnyGt = PunctAny<'>'>;

    unsynn! {
        pub struct Attribute {
            pub pound: Pound,
            pub body: BracketGroup,
        }

        pub struct Visibility {
            pub kw: KwPub,
            pub restriction: Option<ParenthesisGroup>,
        }

        /// One token tree, treating `<...>` as a nested unit.
        pub enum AngleTokenTree {
            Arrow(Cons<PunctAny<'-'>, AnyGt>),
            Nested(Cons<AnyLt, Vec<AngleTokenTree>, AnyGt>),
            Token(Cons<Except<Either<AnyLt, AnyGt>>, TokenTree>),
        }

        pub struct Field {
            pub attrs: Vec<Attribute>,
            pub vis: Option<Visibility>,
            pub name: Ident,
            pub colon: Colon,
            pub ty: Vec<Cons<Except<Comma>, AngleTokenTree>>,
        }

        pub struct Generics {
            pub lt: AnyLt,
            pub params: Vec<AngleTokenTree>,
            pub gt: AnyGt,
        }

        pub struct WhereClause {
            pub kw: KwWhere,
            pub predicates: Vec<Cons<Except<BraceGroup>, AngleTokenTree>>,
        }

        pub struct DeriveStruct {
            pub attrs: Vec<Attribute>,
            pub vis: Option<Visibility>,
            pub kw: KwStruct,
            pub name: Ident,
            pub generics: Option<Generics>,
            pub where_clause: Option<WhereClause>,
            pub fields: BraceGroupContaining<CommaDelimitedVec<Field>>,
        }
    }
}
