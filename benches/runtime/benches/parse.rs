//! Runtime cost of parsing: `syn` vs `parsyng` vs `unsynn` vs `moxy`.
//!
//! Inputs are lexed once outside the measured loop; every parser then gets a
//! clone of its native token stream, as a proc macro receiving its input would
//! (`proc_macro2` for syn and unsynn, `parsyng-fallback` for parsyng, moxy's
//! own tokens for moxy).
//! unsynn has no Rust grammar: it only takes part in `derive_input`, with a
//! minimal hand-written grammar (see `parsyng_bench_runtime::unsynn_grammar`).
//!
//! moxy 0.5 cannot parse `crate::`/`super::`/`self::` paths, so it fails on
//! every whole real-world file. The `parse/common/*` groups therefore re-run
//! each file restricted to the top-level items moxy accepts, so all three full
//! Rust parsers can be compared on identical input; `roundtrip/*` (parse
//! then re-emit tokens) uses the same subset.

use std::hint::black_box;
use std::panic::{AssertUnwindSafe, catch_unwind};

use criterion::{
    BenchmarkGroup, Criterion, criterion_group, criterion_main, measurement::WallTime,
};
use moxy::token::ToTokenStream as _;
use parsyng::ToTokens as _;
use parsyng::ast::crate_source::Crate;
use parsyng::ast::item::DeriveInput;
use parsyng::parse::parse_all;
use parsyng_bench_runtime::{DERIVE_INPUT, FILES, common_subset, unsynn_grammar::DeriveStruct};
use quote::ToTokens as _;
use unsynn::{IParse as _, ToTokens as _};

type Pm2 = proc_macro2::TokenStream;
type Moxy = moxy::token::TokenStream;
type Parsyng = parsyng::proc_macro::TokenStream;

fn lex(src: &str) -> (Pm2, Parsyng, Moxy) {
    (
        src.parse().unwrap(),
        src.parse().unwrap(),
        src.parse().unwrap(),
    )
}

/// Registers a bench only if `f` succeeds once; parsers that `todo!()`/fail
/// on an input are reported and skipped instead of aborting the whole run.
fn bench_if_ok<T>(
    g: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    f: impl Fn() -> Result<T, String>,
) {
    match catch_unwind(AssertUnwindSafe(&f)) {
        Ok(Ok(_)) => {
            g.bench_function(name, |b| b.iter(|| black_box(f())));
        }
        Ok(Err(e)) => eprintln!("skipping {name}: parse error: {e}"),
        Err(_) => eprintln!("skipping {name}: parser panicked"),
    }
}

fn tokenize(c: &mut Criterion) {
    let inputs = std::iter::once(("derive_input", DERIVE_INPUT)).chain(FILES.iter().copied());
    for (name, src) in inputs {
        let mut g = c.benchmark_group(format!("tokenize/{name}"));
        g.bench_function("proc-macro2", |b| {
            b.iter(|| black_box(src.parse::<Pm2>().unwrap()))
        });
        g.bench_function("parsyng", |b| {
            b.iter(|| black_box(src.parse::<Parsyng>().unwrap()))
        });
        g.bench_function("moxy", |b| {
            b.iter(|| black_box(src.parse::<Moxy>().unwrap()))
        });
        g.finish();
    }
}

fn derive_input(c: &mut Criterion) {
    let (pm2, ps, mx) = lex(DERIVE_INPUT);
    let mut g = c.benchmark_group("parse/derive_input");
    bench_if_ok(&mut g, "syn", || {
        syn::parse2::<syn::DeriveInput>(pm2.clone()).map_err(|e| e.to_string())
    });
    bench_if_ok(&mut g, "parsyng", || {
        parse_all::<DeriveInput>(ps.clone()).map_err(|e| format!("{e:?}"))
    });
    bench_if_ok(&mut g, "unsynn", || {
        pm2.clone()
            .into_token_iter()
            .parse_all::<DeriveStruct>()
            .map_err(|e| e.to_string())
    });
    bench_if_ok(&mut g, "moxy", || {
        moxy::parse!(mx as moxy::ast::ItemStruct).map_err(|e| e.to_string())
    });
    g.finish();
}

fn files(c: &mut Criterion) {
    for &(name, src) in FILES {
        let (pm2, ps, mx) = lex(src);
        let mut g = c.benchmark_group(format!("parse/file/{name}"));
        g.sample_size(30);
        bench_if_ok(&mut g, "syn", || {
            syn::parse2::<syn::File>(pm2.clone()).map_err(|e| e.to_string())
        });
        bench_if_ok(&mut g, "parsyng", || {
            parse_all::<Crate>(ps.clone()).map_err(|e| format!("{e:?}"))
        });
        bench_if_ok(&mut g, "moxy", || {
            moxy::parse!(mx as moxy::ast::File).map_err(|e| e.to_string())
        });
        g.finish();
    }
}

fn common(c: &mut Criterion) {
    for &(name, src) in FILES {
        let subset = common_subset(src);
        let (pm2, ps, mx) = lex(&subset);
        let mut g = c.benchmark_group(format!("parse/common/{name}"));
        g.sample_size(30);
        bench_if_ok(&mut g, "syn", || {
            syn::parse2::<syn::File>(pm2.clone()).map_err(|e| e.to_string())
        });
        bench_if_ok(&mut g, "parsyng", || {
            parse_all::<Crate>(ps.clone()).map_err(|e| format!("{e:?}"))
        });
        bench_if_ok(&mut g, "moxy", || {
            moxy::parse!(mx as moxy::ast::File).map_err(|e| e.to_string())
        });
        g.finish();
    }
}

fn roundtrip(c: &mut Criterion) {
    for &(name, src) in FILES {
        let (pm2, ps, mx) = lex(&common_subset(src));
        let mut g = c.benchmark_group(format!("roundtrip/{name}"));
        g.sample_size(30);
        bench_if_ok(&mut g, "syn", || {
            syn::parse2::<syn::File>(pm2.clone())
                .map(|f| f.into_token_stream())
                .map_err(|e| e.to_string())
        });
        bench_if_ok(&mut g, "parsyng", || {
            parse_all::<Crate>(ps.clone())
                .map(|f| f.to_token_stream())
                .map_err(|e| format!("{e:?}"))
        });
        bench_if_ok(&mut g, "moxy", || {
            moxy::parse!(mx as moxy::ast::File)
                .map(|f| f.to_token_stream())
                .map_err(|e| e.to_string())
        });
        g.finish();
    }
}

criterion_group!(benches, tokenize, derive_input, files, common, roundtrip);
criterion_main!(benches);
