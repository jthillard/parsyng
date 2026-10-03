// Inspired from https://github.com/dtolnay/syn

use heapsize::HeapSize;

#[derive(HeapSize)]
struct Demo<'a, T: ?Sized> {
    a: Box<T>,
    b: u8,
    c: &'a str,
    d: String,
}

#[derive(HeapSize)]
struct Pair(String, u8);

#[derive(HeapSize)]
enum Shape<T> {
    Named { name: String, extra: T },
    Pair(Pair),
    Empty,
}

fn main() {
    let demo = Demo {
        a: b"bytestring".to_vec().into_boxed_slice(),
        b: 255,
        c: "&'static str",
        d: "String".to_owned(),
    };

    // 10 + 0 + 0 + 6 = 16
    println!(
        "heap size = {} + {} + {} + {} = {}",
        demo.a.heap_size_of_children(),
        demo.b.heap_size_of_children(),
        demo.c.heap_size_of_children(),
        demo.d.heap_size_of_children(),
        demo.heap_size_of_children()
    );

    assert_eq!(demo.heap_size_of_children(), 16);

    let pair = Pair("four".to_owned(), 1);
    assert_eq!(pair.heap_size_of_children(), 4);

    let named = Shape::Named {
        name: "abc".to_owned(),
        extra: "de".to_owned(),
    };
    assert_eq!(named.heap_size_of_children(), 5);
    assert_eq!(Shape::<String>::Pair(pair).heap_size_of_children(), 4);
    assert_eq!(Shape::<String>::Empty.heap_size_of_children(), 0);
}
