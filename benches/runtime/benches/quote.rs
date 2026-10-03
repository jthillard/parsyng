//! Runtime cost of quasi-quoting: `quote` vs `parsyng` vs `unsynn` vs `moxy`.
//!
//! Each library builds its own native token stream (proc-macro2 for quote and
//! unsynn, `parsyng-fallback` for parsyng, moxy's own `TokenStream`). Each
//! library lives in its own module because their `quote!` expansions need
//! their own `ToTokens` trait in scope.

#![recursion_limit = "512"]

use criterion::{Criterion, criterion_group, criterion_main};

const REPETITIONS: usize = 100;

mod quote_crate {
    use std::hint::black_box;

    use criterion::BenchmarkGroup;
    use criterion::measurement::WallTime;
    use parsyng_bench_runtime::{big_template, field_names, small_template};
    use quote::{format_ident, quote};

    pub fn empty(g: &mut BenchmarkGroup<'_, WallTime>) {
        g.bench_function("quote", |b| b.iter(|| black_box(quote! {})));
    }

    pub fn small(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Bench");
        g.bench_function("quote", |b| {
            b.iter(|| black_box(small_template!(quote, (#ident))));
        });
    }

    pub fn big(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Response");
        g.bench_function("quote", |b| b.iter(|| black_box(big_template!(quote, (#ident)))));
    }

    pub fn big_to_string(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Response");
        g.bench_function("quote", |b| {
            b.iter(|| black_box(big_template!(quote, (#ident)).to_string()));
        });
    }

    pub fn repetition(g: &mut BenchmarkGroup<'_, WallTime>) {
        let names: Vec<_> = field_names(super::REPETITIONS)
            .iter()
            .map(|n| format_ident!("{n}"))
            .collect();
        let ty = format_ident!("u32");
        g.bench_function("quote", |b| {
            b.iter(|| black_box(quote! { struct S { #(#names: #ty,)* } }));
        });
    }
}

mod parsyng_crate {
    use std::hint::black_box;

    use criterion::BenchmarkGroup;
    use criterion::measurement::WallTime;
    use parsyng_bench_runtime::{big_template, field_names, small_template};
    use parsyng::{format_ident, quote};

    pub fn empty(g: &mut BenchmarkGroup<'_, WallTime>) {
        g.bench_function("parsyng", |b| b.iter(|| black_box(quote! {})));
    }

    pub fn small(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Bench");
        g.bench_function("parsyng", |b| {
            b.iter(|| black_box(small_template!(quote, (#ident))));
        });
    }

    pub fn big(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Response");
        g.bench_function("parsyng", |b| b.iter(|| black_box(big_template!(quote, (#ident)))));
    }

    pub fn big_to_string(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Response");
        g.bench_function("parsyng", |b| {
            b.iter(|| black_box(big_template!(quote, (#ident)).to_string()));
        });
    }

    pub fn repetition(g: &mut BenchmarkGroup<'_, WallTime>) {
        let names: Vec<_> = field_names(super::REPETITIONS)
            .iter()
            .map(|n| format_ident!("{n}"))
            .collect();
        let ty = format_ident!("u32");
        g.bench_function("parsyng", |b| {
            b.iter(|| {
                // parsyng repetitions consume iterators.
                let mut names = names.iter();
                let mut ty = std::iter::repeat(&ty);
                black_box(quote! { struct S { #(#names: #ty,)* } })
            });
        });
    }
}

mod unsynn_crate {
    use std::hint::black_box;

    use criterion::BenchmarkGroup;
    use criterion::measurement::WallTime;
    use parsyng_bench_runtime::{big_template, field_names, small_template};
    #[allow(unused_imports)] // only used by `quote!`'s expansion
    use unsynn::ToTokens;
    use unsynn::{format_ident, quote};

    pub fn empty(g: &mut BenchmarkGroup<'_, WallTime>) {
        g.bench_function("unsynn", |b| b.iter(|| black_box(quote! {})));
    }

    pub fn small(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Bench");
        g.bench_function("unsynn", |b| {
            b.iter(|| black_box(small_template!(quote, (#ident))));
        });
    }

    pub fn big(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Response");
        g.bench_function("unsynn", |b| b.iter(|| black_box(big_template!(quote, (#ident)))));
    }

    pub fn big_to_string(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = format_ident!("Response");
        g.bench_function("unsynn", |b| {
            b.iter(|| black_box(big_template!(quote, (#ident)).to_string()));
        });
    }

    pub fn repetition(g: &mut BenchmarkGroup<'_, WallTime>) {
        let names: Vec<_> = field_names(super::REPETITIONS)
            .iter()
            .map(|n| format_ident!("{n}"))
            .collect();
        let ty = format_ident!("u32");
        g.bench_function("unsynn", |b| {
            // unsynn has no `#(...)*`; `#{...}` splices an iterator instead.
            b.iter(|| black_box(quote! { struct S { #{ names.iter().map(|n| quote! { #n: #ty, }) } } }));
        });
    }
}

mod moxy_crate {
    use std::hint::black_box;

    use criterion::BenchmarkGroup;
    use criterion::measurement::WallTime;
    use moxy::template;
    use moxy::token::ident;
    use parsyng_bench_runtime::{big_template, field_names, small_template};

    pub fn empty(g: &mut BenchmarkGroup<'_, WallTime>) {
        g.bench_function("moxy", |b| b.iter(|| black_box(template! {})));
    }

    pub fn small(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = ident!("Bench");
        g.bench_function("moxy", |b| {
            b.iter(|| black_box(small_template!(template, ({{ ident }}))));
        });
    }

    pub fn big(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = ident!("Response");
        g.bench_function("moxy", |b| {
            b.iter(|| black_box(big_template!(template, ({{ ident }}))));
        });
    }

    pub fn big_to_string(g: &mut BenchmarkGroup<'_, WallTime>) {
        let ident = ident!("Response");
        g.bench_function("moxy", |b| {
            b.iter(|| black_box(big_template!(template, ({{ ident }})).to_string()));
        });
    }

    pub fn repetition(g: &mut BenchmarkGroup<'_, WallTime>) {
        let names: Vec<_> = field_names(super::REPETITIONS)
            .iter()
            .map(|n| ident!(n.as_str()))
            .collect();
        let ty = ident!("u32");
        g.bench_function("moxy", |b| {
            b.iter(|| black_box(template! { struct S { @for n in &names { {{ n }}: {{ ty }}, } } }));
        });
    }
}

macro_rules! cases {
    ($($case:ident),* $(,)?) => {
        $(
            fn $case(c: &mut Criterion) {
                let mut g = c.benchmark_group(concat!("quote/", stringify!($case)));
                quote_crate::$case(&mut g);
                parsyng_crate::$case(&mut g);
                unsynn_crate::$case(&mut g);
                moxy_crate::$case(&mut g);
                g.finish();
            }
        )*
        criterion_group!(benches, $($case),*);
    };
}

cases!(empty, small, big, big_to_string, repetition);
criterion_main!(benches);
