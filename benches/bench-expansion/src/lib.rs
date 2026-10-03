//! Derives `HeapSize` on many types (see `gen.sh`), so building this crate
//! measures macro expansion time with whichever library feature is enabled.

#![allow(missing_docs)]

use bench_comptime::HeapSize;

/// Heap memory owned by a value, excluding the value itself.
pub trait HeapSize {
    fn heap_size_of_children(&self) -> usize;
}

macro_rules! no_heap {
    ($($t:ty),*) => {
        $(impl HeapSize for $t {
            fn heap_size_of_children(&self) -> usize { 0 }
        })*
    };
}
no_heap!(u8, u32, u64);

impl HeapSize for String {
    fn heap_size_of_children(&self) -> usize {
        self.capacity()
    }
}

impl<T: HeapSize> HeapSize for Vec<T> {
    fn heap_size_of_children(&self) -> usize {
        self.iter().map(HeapSize::heap_size_of_children).sum::<usize>()
            + self.capacity() * size_of::<T>()
    }
}

impl<T: HeapSize> HeapSize for Option<T> {
    fn heap_size_of_children(&self) -> usize {
        self.as_ref().map_or(0, HeapSize::heap_size_of_children)
    }
}

impl<T: HeapSize> HeapSize for Box<T> {
    fn heap_size_of_children(&self) -> usize {
        size_of::<T>() + (**self).heap_size_of_children()
    }
}

impl<T: HeapSize, const N: usize> HeapSize for [T; N] {
    fn heap_size_of_children(&self) -> usize {
        self.iter().map(HeapSize::heap_size_of_children).sum()
    }
}

impl<T: ?Sized> HeapSize for &T {
    fn heap_size_of_children(&self) -> usize {
        0
    }
}

include!("generated.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_impls_sum_fields() {
        let tuple = Tuple1(String::with_capacity(8), vec![1, 2], 3, None);
        assert_eq!(tuple.heap_size_of_children(), 8 + 2);
        let generic = Generic2::<'_, u8, String, 2> {
            name: "x",
            value: 0,
            values: vec![String::with_capacity(4)],
            array: [0; 2],
        };
        assert_eq!(generic.heap_size_of_children(), 4 + size_of::<String>());
        assert_eq!(Unit3.heap_size_of_children(), 0);
    }
}
