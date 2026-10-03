//! Keyword and punctuation token types, and the [`Token!`](crate::Token)
//! macro used to name them.
//!
//! Every Rust keyword (`struct`, `fn`, `where`, ...) and every 1-to-3
//! character punctuation sequence (`+`, `::`, `..=`, ...) has a
//! corresponding zero-or-few-field type here (e.g. [`StructKeyword`],
//! [`Plus`], [`PathSep`], [`DotDotEq`]) that implements
//! [`Parse`]/[`ToTokens`], and [`Peek`] where matching it can never consume
//! input on failure. Rather than write the type name directly, most code
//! (including elsewhere in this crate) spells it with the
//! [`Token!`](crate::Token) macro, matching the punctuation or keyword's
//! surface syntax:
//!
//! ```ignore
//! use parsyng::Token;
//!
//! fn parse_pub(input: &mut parsyng::parse::ParseBuffer) -> parsyng::error::Result<()> {
//!     input.parse::<Token![pub]>()?;
//!     input.parse::<Token![+]>()?;
//!     Ok(())
//! }
//! ```
//!
//! `Token![struct]` expands to [`StructKeyword`], `Token![+]` expands to
//! [`Plus`], and so on for all 53 keywords and ~45 punctuation sequences
//! defined by the `make_tokens!` invocation below.
//!
//! All of these type aliases are backed by just two generic types,
//! parameterized over `const` values rather than generated one struct at a
//! time: [`RustKeyword<K>`] (keyword, selected by table index `K`) and
//! [`RustPunct1`]/[`RustPunct2`]/[`RustPunct3`] (1-, 2- and 3-character
//! punctuation, selected by the `char`s themselves).
//!
//! Reference: <https://doc.rust-lang.org/reference/tokens.html#punctuation> and
//! <https://doc.rust-lang.org/reference/keywords.html>

use crate::ToTokens;

use crate::{
    error::{Diagnostics, Result},
    parse::{Parse, ParseBuffer, Peek},
    proc_macro::{Ident, Punct, Spacing, Span},
};

macro_rules! make_tokens {
    (@keywords $($keyword:ident $i:literal => $keyword_name:ident)* @puncts $($punct:tt $($lit:literal),* => $punct_name:ident #[doc = $punct_usage:literal])*) => {
        /// Names a keyword or punctuation token type by its surface syntax. See the [module docs](crate::ast::tokens)
        /// for details.
        ///
        /// ```ignore
        /// Token![struct] // == StructKeyword
        /// Token![+]      // == Plus
        /// ```
        ///
        /// Reference: <https://doc.rust-lang.org/reference/tokens.html#punctuation> and
        /// <https://doc.rust-lang.org/reference/keywords.html>
        #[macro_export]
        macro_rules! Token {
            $(
                ($keyword) => {
                    $crate::ast::tokens::$keyword_name
                };
            )*
            $(
                ($punct) => {
                    $crate::ast::tokens::$punct_name
                };
            )*
        }

        make_keywords! {
            $($keyword $i => $keyword_name)*
        }

        make_puncts! {
            $($punct $($lit)* => $punct_name #[doc = $punct_usage])*
        }
    };
}
macro_rules! make_puncts {
    (@type $a:literal) => {
        RustPunct1<$a>
    };
    (@type $a:literal $b:literal) => {

        RustPunct2<$a, $b>
    };
    (@type $a:literal $b:literal $c:literal) => {

        RustPunct3<$a, $b, $c>
    };
    ($($t:tt $($lit:literal)* => $name:ident #[doc = $usage:literal])*) => {
        $(
            #[doc = $usage]
            #[doc = ""]
            #[doc = "Reference: <https://doc.rust-lang.org/reference/tokens.html#punctuation>"]
            pub type $name = make_puncts!(@type $($lit)*);
        )*
    };
}
macro_rules! make_keywords {
    ($($keyword:tt $i:literal => $name:ident)*) => {
        /// `"Expected keyword `...`"`, indexed like [`RustKeyword`]'s `K`.
        const KEYWORD_ERRORS: [&'static str; 53] =
            [$(concat!("Expected keyword `", stringify!($keyword), "`")),*];

        /// The index of the keyword `text` spells, or [`NOT_A_KEYWORD`].
        pub(crate) fn keyword_index(text: &str) -> u8 {
            match text {
                $(stringify!($keyword) => $i,)*
                _ => NOT_A_KEYWORD,
            }
        }

        $(
            #[doc = concat!("`", stringify!($keyword), "` keyword")]
            #[doc = ""]
            #[doc = "Reference: <https://doc.rust-lang.org/reference/keywords.html>"]
            pub type $name = RustKeyword<$i>;
        )*
    };
}

make_tokens! {
    @keywords

    abstract 0  => Abstract
    as       1  => As
    async    2  => Async
    auto     3  => Auto
    await    4  => Await
    become   5  => Become
    box      6  => Box
    break    7  => Break
    const    8  => Const
    continue 9  => Continue
    crate    10 => Crate
    default  11 => Default
    do       12 => Do
    dyn      13 => Dyn
    else     14 => Else
    enum     15 => Enum
    extern   16 => Extern
    final    17 => Final
    fn       18 => Fn
    for      19 => For
    if       20 => If
    impl     21 => Impl
    in       22 => In
    let      23 => Let
    loop     24 => Loop
    macro    25 => Macro
    match    26 => Match
    mod      27 => Mod
    move     28 => Move
    mut      29 => Mut
    override 30 => Override
    priv     31 => Priv
    pub      32 => Pub
    raw      33 => Raw
    ref      34 => Ref
    return   35 => Return
    Self     36 => SelfType
    self     37 => SelfValue
    static   38 => Static
    struct   39 => StructKeyword
    super    40 => Super
    trait    41 => Trait
    try      42 => Try
    type     43 => Type
    typeof   44 => Typeof
    union    45 => Union
    unsafe   46 => Unsafe
    unsized  47 => Unsized
    use      48 => Use
    virtual  49 => Virtual
    where    50 => Where
    while    51 => While
    yield    52 => Yield

    @puncts

    &     '&'           => And        /// bitwise and logical AND, borrow, references, reference patterns
    &&    '&', '&'      => AndAnd     /// lazy AND, borrow, references, reference patterns
    &=    '&', '='      => AndEq      /// bitwise AND assignment
    @     '@'           => At         /// subpattern binding
    ^     '^'           => Caret      /// bitwise and logical XOR
    ^=    '^', '='      => CaretEq    /// bitwise XOR assignment
    :     ':'           => Colon      /// various separators
    ,     ','           => Comma      /// various separators
    $     '$'           => Dollar     /// macros
    .     '.'           => Dot        /// field access, tuple index
    ..    '.', '.'      => DotDot     /// range, struct expressions, patterns, range patterns
    ...   '.', '.', '.' => DotDotDot  /// variadic functions, range patterns
    ..=   '.', '.', '=' => DotDotEq   /// inclusive range, range patterns
    =     '='           => Eq         /// assignment, attributes, various type definitions
    ==    '=', '='      => EqEq       /// equal
    =>    '=', '>'      => FatArrow   /// match arms, macros
    >=    '>', '='      => Ge         /// greater than or equal to, generics
    >     '>'           => Gt         /// greater than, generics, paths
    <-    '<', '-'      => LArrow     /// unused
    <=    '<', '='      => Le         /// less than or equal to
    <     '<'           => Lt         /// less than, generics, paths
    -     '-'           => Minus      /// subtraction, negation
    -=    '-', '='      => MinusEq    /// subtraction assignment
    !=    '!', '='      => Ne         /// not equal
    !     '!'           => Not        /// bitwise and logical NOT, macro calls, inner attributes, never type, negative impls
    |     '|'           => Or         /// bitwise and logical OR, closures, patterns in match, if let, and while let
    |=    '|', '='      => OrEq       /// bitwise OR assignment
    ||    '|', '|'      => OrOr       /// lazy OR, closures
    ::    ':', ':'      => PathSep    /// path separator
    %     '%'           => Percent    /// remainder
    %=    '%', '='      => PercentEq  /// remainder assignment
    +     '+'           => Plus       /// addition, trait bounds, macro Kleene matcher
    +=    '+', '='      => PlusEq     /// addition assignment
    #     '#'           => Pound      /// attributes
    ?     '?'           => Question   /// question mark operator, questionably sized, macro Kleene matcher
    ->    '-', '>'      => RArrow     /// function return type, closure return type, function pointer type
    ;     ';'           => Semicolon  /// terminator for various items and statements, array types
    <<    '<', '<'      => Shl        /// shift left, nested generics
    <<=   '<', '<', '=' => ShlEq      /// shift left assignment
    >>    '>', '>'      => Shr        /// shift right, nested generics
    >>=   '>', '>', '=' => ShrEq      /// shift right assignment, nested generics
    /     '/'           => Slash      /// division
    /=    '/', '='      => SlashEq    /// division assignment
    *     '*'           => Star       /// multiplication, dereference, raw pointers, macro Kleene matcher, use wildcards
    *=    '*', '='      => StarEq     /// multiplication assignment
    ~     '~'           => Tilde      /// unused since before Rust 1.0
    quote '\''          => Quote      /// single quotes, used in lifetimes
}

/// The generic type backing every keyword token alias (see the
/// [module docs](self)), such as [`StructKeyword`] (`RustKeyword<39>`) or
/// [`Pub`] (`RustKeyword<32>`).
///
/// `K` indexes into an internal table of the 53 Rust keywords; parsing
/// succeeds only if the next identifier's text matches that keyword exactly.
/// There is normally no need to name `RustKeyword<K>` directly — use the
/// keyword's alias, or spell it with [`Token!`](crate::Token) (`Token![struct]`).
///
/// Reference: <https://doc.rust-lang.org/reference/keywords.html>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct RustKeyword<const K: u8> {
    ident: Ident,
}

/// Returned by `keyword_index` for identifiers that are not keywords.
pub(crate) const NOT_A_KEYWORD: u8 = u8::MAX;

impl<const K: u8> Parse for RustKeyword<K> {
    #[inline]
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        match input.keyword(K) {
            Some(ident) => Ok(Self { ident }),
            None => Err(keyword_error(input, K)),
        }
    }
}

#[cold]
fn keyword_error(input: &ParseBuffer, keyword: u8) -> Diagnostics {
    Diagnostics::new_error_spanned(KEYWORD_ERRORS[keyword as usize], input.span())
}
impl<const K: u8> RustKeyword<K> {
    /// This keyword's span.
    #[must_use]
    pub fn span(&self) -> Span {
        self.ident.span()
    }
}

impl<const K: u8> Peek for RustKeyword<K> {
    #[inline]
    fn peek(input: &ParseBuffer) -> bool {
        input.peek_keyword() == Some(K)
    }
}
impl<const K: u8> ToTokens for RustKeyword<K> {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        tokens.extend(Some(self.ident.clone()));
    }
}

/// The generic type backing every 1-character punctuation alias, such as
/// [`Plus`] (`RustPunct1<'+'>`) or [`Comma`] (`RustPunct1<','>`).
///
/// See the [module docs](self). There is normally no need to name it
/// directly — use the punctuation's alias, or [`Token!`](crate::Token)
/// (`Token![+]`).
///
/// Reference: <https://doc.rust-lang.org/reference/tokens.html#punctuation>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct RustPunct1<const A: char>([Punct; 1]);

/// The generic type backing every 2-character punctuation alias, such as
/// [`PathSep`] (`RustPunct2<':', ':'>`) or [`FatArrow`] (`RustPunct2<'=', '>'>`).
///
/// See the [module docs](self). There is normally no need to name it
/// directly — use the punctuation's alias, or [`Token!`](crate::Token)
/// (`Token![::]`).
///
/// Reference: <https://doc.rust-lang.org/reference/tokens.html#punctuation>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct RustPunct2<const A: char, const B: char>([Punct; 2]);

/// The generic type backing every 3-character punctuation alias, such as
/// [`DotDotEq`] (`RustPunct3<'.', '.', '='>`).
///
/// See the [module docs](self). There is normally no need to name it
/// directly — use the punctuation's alias, or [`Token!`](crate::Token)
/// (`Token![..=]`).
///
/// Reference: <https://doc.rust-lang.org/reference/tokens.html#punctuation>
#[derive(Clone)]
#[cfg_attr(feature = "extra-traits", derive(Debug))]
pub struct RustPunct3<const A: char, const B: char, const C: char>([Punct; 3]);

impl<const A: char> RustPunct1<A> {
    /// Build this punctuation token spanned at `span`.
    #[must_use]
    pub fn new(span: Span) -> Self {
        let mut punct = Punct::new(A, Spacing::Alone);
        punct.set_span(span);
        Self([punct])
    }
    /// This token's span.
    #[must_use]
    pub fn span(&self) -> Span {
        self.0[0].span()
    }
    /// The span of the single underlying [`Punct`].
    #[must_use]
    pub fn spans(&self) -> [Span; 1] {
        self.0.clone().map(|punct| punct.span())
    }
}

impl<const A: char, const B: char> RustPunct2<A, B> {
    /// Build this punctuation token spanned at `span`.
    #[must_use]
    pub fn new(span: Span) -> Self {
        let mut punct1 = Punct::new(A, Spacing::Joint);
        let mut punct2 = Punct::new(B, Spacing::Alone);
        punct1.set_span(span);
        punct2.set_span(span);
        Self([punct1, punct2])
    }
    /// The spans of the two underlying [`Punct`]s.
    #[must_use]
    pub fn spans(&self) -> [Span; 2] {
        self.0.clone().map(|punct| punct.span())
    }
}

impl<const A: char, const B: char, const C: char> RustPunct3<A, B, C> {
    /// Build this punctuation token spanned at `span`.
    #[must_use]
    pub fn new(span: Span) -> Self {
        let mut punct1 = Punct::new(A, Spacing::Joint);
        let mut punct2 = Punct::new(B, Spacing::Joint);
        let mut punct3 = Punct::new(C, Spacing::Alone);
        punct1.set_span(span);
        punct2.set_span(span);
        punct3.set_span(span);
        Self([punct1, punct2, punct3])
    }
    /// The spans of the three underlying [`Punct`]s.
    #[must_use]
    pub fn spans(&self) -> [Span; 3] {
        self.0.clone().map(|punct| punct.span())
    }
}

impl<const A: char> Parse for RustPunct1<A> {
    #[inline]
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        match input.punct_char_and(|ch, _| ch == A) {
            Some(punct) => Ok(Self([punct])),
            None => Err(punct_error(input, &[A])),
        }
    }
}

/// `"Expected token `...`"`, only formatted if the error is ever displayed.
#[cold]
fn punct_error(input: &ParseBuffer, chars: &[char]) -> Diagnostics {
    Diagnostics::expected_token(chars, input.span())
}

impl<const A: char> Peek for RustPunct1<A> {
    #[inline]
    fn peek(input: &ParseBuffer) -> bool {
        input.peek_punct_char().is_some_and(|(ch, _)| ch == A)
    }
}

impl<const A: char> ToTokens for RustPunct1<A> {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.0[0].to_tokens(tokens);
    }
}
impl<const A: char, const B: char> Parse for RustPunct2<A, B> {
    #[inline]
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if input.nth_punct_char(0) == Some((A, true))
            && input.nth_punct_char(1).is_some_and(|(ch, _)| ch == B)
            && let (Some(punct1), Some(punct2)) = (input.punct(), input.punct())
        {
            return Ok(Self([punct1, punct2]));
        }
        Err(punct_error(input, &[A, B]))
    }
}
impl<const A: char, const B: char> Peek for RustPunct2<A, B> {
    #[inline]
    fn peek(input: &ParseBuffer) -> bool {
        input.nth_punct_char(0) == Some((A, true))
            && input.nth_punct_char(1).is_some_and(|(ch, _)| ch == B)
    }
}

impl<const A: char, const B: char> ToTokens for RustPunct2<A, B> {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.0[0].to_tokens(tokens);
        self.0[1].to_tokens(tokens);
    }
}

impl<const A: char, const B: char, const C: char> Parse for RustPunct3<A, B, C> {
    #[inline]
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if input.nth_punct_char(0) == Some((A, true))
            && input.nth_punct_char(1) == Some((B, true))
            && input.nth_punct_char(2).is_some_and(|(ch, _)| ch == C)
            && let (Some(punct1), Some(punct2), Some(punct3)) =
                (input.punct(), input.punct(), input.punct())
        {
            return Ok(Self([punct1, punct2, punct3]));
        }
        Err(punct_error(input, &[A, B, C]))
    }
}
impl<const A: char, const B: char, const C: char> Peek for RustPunct3<A, B, C> {
    #[inline]
    fn peek(input: &ParseBuffer) -> bool {
        input.nth_punct_char(0) == Some((A, true))
            && input.nth_punct_char(1) == Some((B, true))
            && input.nth_punct_char(2).is_some_and(|(ch, _)| ch == C)
    }
}

impl<const A: char, const B: char, const C: char> ToTokens for RustPunct3<A, B, C> {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.0[0].to_tokens(tokens);
        self.0[1].to_tokens(tokens);
        self.0[2].to_tokens(tokens);
    }
}
