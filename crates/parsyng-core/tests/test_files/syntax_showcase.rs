//! A hand-written tour of the stable syntax `parsyng` supports, round-tripped
//! as a `Crate` by `tests/parse_crates.rs`. Nightly syntax is covered by the
//! unit tests in `src/ast/tests/`.
#![allow(dead_code, unused)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;
extern crate self as showcase;

use ::core::fmt;
use alloc::{
    boxed::Box,
    string::{String, ToString as _},
    vec::{self, Vec},
};
use std::collections::*;
pub(crate) use std::sync::{Arc, Mutex as Lock};

/// A unit struct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Unit;

/// A tuple struct with attributes on its fields.
pub struct Tuple<'a, T: ?Sized + 'a>(#[doc = "first"] pub &'a T, pub(crate) usize);

/// A struct with every kind of generic parameter.
#[repr(C)]
pub struct Generic<'a, 'b: 'a, #[cfg(all())] T, U = u8, const N: usize = 3>
where
    T: Clone + fmt::Debug + 'a,
    for<'c> &'c U: IntoIterator,
{
    pub(in crate::inner) field: [T; N],
    pub(super) reference: &'a mut Option<&'b U>,
    pointer: *const [u8],
    function: fn(u8, ...) -> !,
    unsafe_function: unsafe extern "C" fn(*mut u8),
    closure: Box<dyn Fn(&str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + 'a>,
    tuple: (u8, (u16,), ()),
    qualified: <Vec<T> as IntoIterator>::IntoIter,
    impl_trait_free: core::marker::PhantomData<fn() -> T>,
}

/// An enum with every variant shape.
#[non_exhaustive]
pub enum Shape<T> {
    #[default]
    Unit,
    Tuple(#[allow(unused)] T, u8),
    Named { x: T, #[doc = "y"] y: i64 },
    Discriminant = 1 << 4,
    Block = { 3 + 4 },
}

/// A union.
#[repr(C)]
pub union Bits<T: Copy> {
    pub int: u32,
    float: f32,
    other: T,
}

#[derive(Debug)]
pub enum Error {
    Io(#[source] std::io::Error),
    Message(String),
}

pub const CONSTANT: u32 = 0x_FF_u32 + 0o17 + 0b1010 + 1_000;
const _: () = assert!(CONSTANT > 0);
pub static mut COUNTER: usize = 0;
static NAME: &str = "showcase";
static BYTES: &[u8] = b"bytes\x00\n";
static RAW: &str = r#"raw "string""#;
static C_STR: &core::ffi::CStr = c"c string";

pub type Alias<T> = Result<T, Error>;
type Generic2<'a, T> where T: 'a = &'a [T];

pub trait Trait<Rhs = Self>: Clone + Sized
where
    Rhs: ?Sized,
{
    /// Associated type with bounds and a where clause.
    type Output<'a>: Iterator<Item: Clone + 'a>
    where
        Self: 'a;
    type Defaulted = u8;
    const ID: u32;
    const DEFAULT: u32 = 1;

    fn required(&self, rhs: &Rhs) -> Self::Defaulted;
    fn provided(&mut self) -> Option<u32> where Self: Default {
        Some(Self::DEFAULT)
    }
    async fn asynchronous(self: Box<Self>) {}
    unsafe fn dangerous(&'static self, ptr: *mut u8);
}

pub unsafe trait UnsafeTrait {}
pub auto trait AutoTrait {}

impl<T: Clone> Clone for Tuple<'_, T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0, self.1)
    }
}

unsafe impl<T> UnsafeTrait for Shape<T> {}
impl !AutoTrait for Unit {}
impl<'a, T, const N: usize> Generic<'a, 'a, T, u8, N> where T: Clone + fmt::Debug + 'a {}
impl dyn fmt::Debug {}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", "Unit")
    }
}

mod inner {
    #![allow(missing_docs)]
    pub fn nested() {}
}

mod external;

extern "C" {
    fn abs(x: i32) -> i32;
    static errno: i32;
}

unsafe extern "C" {
    #![allow(non_upper_case_globals)]
    /// Safe to call.
    pub safe fn sqrt(x: f64) -> f64;
    pub unsafe fn free(ptr: *mut u8);
    fn printf(format: *const u8, ...) -> i32;
    pub safe static VERSION: u32;
    unsafe static mut ERRNO: i32;
}

pub struct Handlers {
    callback: fn(code: i32, _: *const u8) -> bool,
    generic: for<'a> fn(&'a str) -> &'a str,
}

pub fn run<F: async Fn(u8) -> u8>(#[allow(unused)] f: F, #[cfg(all())] mut n: u8) {}

impl Unit {
    pub const ZERO: u8 = 0;
    pub(crate) fn new() -> Self {
        Unit
    }
}

macro_rules! paren_macro ( ($x:expr) => { $x } );
macro_rules! bracket_macro [ () => {} ];
macro_rules! brace_macro {
    ($($t:tt)*) => { $($t)* };
}
brace_macro! { fn generated() {} }
paren_macro!(1);
thread_local! {
    static LOCAL: core::cell::Cell<u8> = const { core::cell::Cell::new(0) };
}

pub const fn const_fn<const N: usize>() -> usize {
    N * 2
}

pub async unsafe fn qualifiers() {}

extern "C" fn callback(_: i32, mut __: *const u8) -> i32 {
    0
}

pub fn statements<'a, T>(items: &'a mut Vec<T>, (a, b): (u8, u8)) -> impl Iterator<Item = &'a T> + 'a
where
    T: Clone + PartialEq,
{
    ;
    let x;
    let y: u8;
    x = 1;
    y = x + 1;
    let mut z = (x, y);
    let (ref first, mut second) = z;
    let Some(value) = items.first() else {
        return items.iter();
    };
    let Unit = Unit;
    #[allow(unused_variables)]
    let shadowed = &mut z.0;
    *shadowed += 1;
    struct Local {
        field: u8,
    }
    fn local_fn() -> u8 {
        7
    }
    use std::mem;
    if a > b {
        mem::swap(&mut z.0, &mut z.1);
    } else if let Some(last) = items.last()
        && last == value
    {
        items.pop();
    } else {
        items.clear();
    }
    items.iter()
}

pub fn expressions(input: &[u8], text: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let literals = (1u8, -2i32, 3.5f64, 1e10, 'c', b'b', "s", b"bytes", r"raw", c"c", true, false);
    let array = [1, 2, 3];
    let repeat = [0u8; 32];
    let unit = ();
    let single = (1,);
    let index = array[0] + repeat[1..3][0] as i32;
    let ranges = (.., 1.., ..2, 1..2, ..=3, 1..=3);
    let unary = (-index, !true, *&index, &&index, &mut 1, &raw const index, &raw mut z_val());
    let binary = 1 + 2 * 3 - 4 / 5 % 6 << 1 >> 1 & 7 | 8 ^ 9;
    let comparison = 1 < 2 && 2 <= 3 || 3 > 4 && 4 >= 5 && 5 == 5 && 6 != 7;
    let mut compound = 0;
    compound += 1;
    compound -= 1;
    compound *= 2;
    compound /= 2;
    compound %= 3;
    compound <<= 1;
    compound >>= 1;
    compound &= 1;
    compound |= 1;
    compound ^= 1;
    let cast = compound as u64 as usize;
    let call = usize::from(1u8).max(2).checked_add(3).unwrap_or_default();
    let turbofish = text.parse::<u32>()?;
    let collected = input.iter().copied().map(u32::from).collect::<Vec<_>>();
    let path = <Vec<u8> as Default>::default();
    let qualified = <u8>::MAX;
    let field = Tuple(&1, 2).1;
    let nested_tuple = (((1, 2), 3).0).1;
    let structure = Generic2Literal { a: 1, b: 2 };
    let shorthand = Generic2Literal { a, ..structure };
    let numeric_fields = Tuple { 0: &1, 1: 2 };
    let closure = |x: u8, y| -> u8 { x + y };
    let moved = move || closure(1, 2);
    let async_closure = async move |x: u8| x;
    let nested = |x| |y| x + y;
    let macro_call = vec![1, 2, 3];
    let formatted = format!("{text} {}", 1);
    let block = {
        let inner = 1;
        inner * 2
    };
    let labeled = 'outer: {
        if block > 1 {
            break 'outer 1;
        }
        2
    };
    let unsafe_value = unsafe { *core::ptr::null::<u8>() };
    let constant = const { 1 + 2 };
    let future = async { 1 };
    let looped = loop {
        break 42;
    };
    'outer: for (i, x) in input.iter().enumerate().rev() {
        while let Some(y) = Some(x) {
            if i > 2 {
                continue 'outer;
            }
            break;
        }
    }
    let matched = match input {
        b"" => 0,
        _ if input.len() > 3 => 1,
        bytes => {
            match bytes.first() {
                Some(&b'a' | &b'b') => 2,
                Some(byte @ &0) => usize::from(*byte),
                Some(_) | None => 3,
            }
        }
    };
    let attributed = #[allow(unused_parens)] (1);
    let underscore;
    _ = 1;
    (underscore, _) = (1, 2);
    let try_block = text.len().checked_sub(1).ok_or("empty")?;
    Ok(looped + matched)
}

async fn awaiting(fut: impl core::future::Future<Output = u8>) -> u8 {
    let value = fut.await;
    let chained = async { 1 }.await + value;
    chained
}

fn more_syntax(items: &[u8], value: Option<Result<u8, u8>>) -> usize {
    #![allow(unused)]
    let doubled: Vec<_> = items.iter().map(|&x| x * 2).collect();
    let summed = items.iter().fold(0, |acc, &(x)| acc + usize::from(x));
    let mut first = 0;
    let mut rest = 0;
    [first, rest, ..] = [1, 2, 3];
    Tuple { 0: _, .. } = Tuple(&1, 2);
    let len = vec![1, 2].len() + format!("{}", 1).len();
    let class = match items {
        [] => 0,
        [one] => 1,
        [first, .., last] if first == last => 2,
        [_, rest @ ..] => rest.len(),
    };
    let grade = match items.len() {
        0 => 'z',
        1..=9 => 'a',
        10..100 => 'b',
        100.. => 'c',
    };
    let letter = match 'q' {
        'a'..='m' => 1,
        'n'..='z' | 'A'..='Z' => 2,
        _ => 3,
    };
    let guarded = match value {
        Some(inner) if let Ok(n) = inner && n > 0 => n,
        | Some(_) | None => 0,
    };
    let generic = match Some(1u8) {
        Option::<u8>::Some(x) => x,
        Option::<u8>::None => 0,
    };
    class + len
}

fn patterns(value: Shape<u8>, pair: &(u8, u8), opt: Option<Result<u8, u8>>) {
    match value {
        Shape::Unit => {}
        Shape::Tuple(a, ..) => {}
        Shape::Named { x: 0, y } => {}
        Shape::Tuple { 0: first, 1: _ } => {}
        Shape::Named { x, .. } => {}
        Shape::Discriminant | Shape::Block => {}
    }
    let &(ref a, mut b) = pair;
    let (first, .., last) = (1, 2, 3, 4);
    if let Some(Ok(n) | Err(n)) = opt {}
    match 'c' {
        'a' | 'b' => {}
        c @ _ => {}
    }
    match -1 {
        <i32>::MIN | i32::MAX => {}
        matches_nothing!() => {}
        -1 => {}
        0 => {}
        _ => {}
    }
}
