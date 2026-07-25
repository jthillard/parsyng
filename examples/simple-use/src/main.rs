use simple_use_macros::Simple;
use simple_use_macros::add_one;
use simple_use_macros::simple_macro;
use simple_use_macros::simple_macro_attribute;

fn main() {
    println!("Hello, world!");
    println!("{}", add_one!(5));

    simple_macro! {
        pub(in ::a::test) struct Foo<'a, 'b: '_ + 'static, T: 'a + Test<T> + Bar + ?Sized + (for<T> T)> where 'a: 'b, T: Add {
            field: core::primitive::u8,
            field2: Foo<T>
        }
        unsafe impl Deref for T {
            type Inner<A>:K+'a=A;
            const Q: u8 = 1;
        }
    };
}

#[simple_macro_attribute(88, sen)]
struct _A {}

#[derive(Simple)]
struct _B {}
