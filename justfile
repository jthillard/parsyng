test:
    cargo test --package parsyng-core --features proc-macro2

# Extra arguments passed to every hyperfine run, e.g. `HYPERFINE_ARGS="--runs 3" just bench`.
hyperfine_args := env("HYPERFINE_ARGS", "")
target := justfile_directory() / "target"
results := target / "bench-results"

# Every benchmark; `bench-report` also writes BENCH.md.
bench: bench-comptime bench-quote-comptime bench-expansion bench-runtime

# Run every benchmark, then summarise them in BENCH.md (needs hyperfine and jq).
bench-report: bench
    ./benches/report.sh

# Runtime of quote!/parsing with criterion (results in target/criterion).
bench-runtime:
    cargo bench -p parsyng-bench-runtime

# Clean build of a proc-macro crate implementing the same derive with each library.
bench-comptime: (__comptime "empty") (__comptime "small") (__comptime "big") (__comptime_impl "big" "big,proc-macro2" "big-proc-macro2")

# Clean build of a proc-macro crate expanding the same `quote!` template with each library.
bench-quote-comptime: (__quote_comptime "empty") (__quote_comptime "small") (__quote_comptime "big")

# Build of a crate with 200 `#[derive(HeapSize)]`, the macro itself being already built.
bench-expansion: (__expansion "") (__expansion "--release")

__comptime size: (__comptime_impl size size size)

__comptime_impl size parsyng_features name: (__comptime_profile size parsyng_features name "") (__comptime_profile size parsyng_features name "--release")

__comptime_profile size parsyng_features name profile:
    @echo -e {{ BOLD }}{{ RED }}"\n============ Library compile time: {{ name }} {{ profile }} ============\n"{{ NORMAL }}
    @mkdir -p {{ results }}
    CARGO_TARGET_DIR={{ target }}/bench-comptime hyperfine {{ hyperfine_args }} --prepare 'cargo clean -q' \
        --export-markdown {{ results }}/comptime-{{ name }}{{ if profile == "" { "" } else { "-release" } }}.md \
        -n syn 'cargo build -q {{ profile }} -p bench-comptime --features syn,{{ size }}' \
        -n unsynn 'cargo build -q {{ profile }} -p bench-comptime --features unsynn,{{ size }}' \
        -n moxy 'cargo build -q {{ profile }} -p bench-comptime --features moxy,{{ size }}' \
        -n parsyng 'cargo build -q {{ profile }} -p bench-comptime --features parsyng,{{ parsyng_features }}'

__quote_comptime size: (__quote_comptime_profile size "") (__quote_comptime_profile size "--release")

__quote_comptime_profile size profile:
    @echo -e {{ BOLD }}{{ RED }}"\n============ quote! compile time: {{ size }} {{ profile }} ============\n"{{ NORMAL }}
    @mkdir -p {{ results }}
    CARGO_TARGET_DIR={{ target }}/bench-quote-comptime hyperfine {{ hyperfine_args }} --prepare 'cargo clean -q' \
        --export-markdown {{ results }}/quote-comptime-{{ size }}{{ if profile == "" { "" } else { "-release" } }}.md \
        -n quote 'cargo build -q {{ profile }} -p bench-quote-comptime --features quote,{{ size }}' \
        -n unsynn 'cargo build -q {{ profile }} -p bench-quote-comptime --features unsynn,{{ size }}' \
        -n moxy 'cargo build -q {{ profile }} -p bench-quote-comptime --features moxy,{{ size }}' \
        -n parsyng 'cargo build -q {{ profile }} -p bench-quote-comptime --features parsyng-core,{{ size }}'

__expansion profile:
    @echo -e {{ BOLD }}{{ RED }}"\n============ Macro expansion time {{ profile }} ============\n"{{ NORMAL }}
    @mkdir -p {{ results }}
    CARGO_TARGET_DIR={{ target }}/bench-expansion hyperfine {{ hyperfine_args }} \
        --export-markdown {{ results }}/expansion{{ if profile == "" { "" } else { "-release" } }}.md \
        --prepare 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features syn && cargo clean -q {{ profile }} -p bench-expansion' \
        --prepare 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features unsynn && cargo clean -q {{ profile }} -p bench-expansion' \
        --prepare 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features moxy && cargo clean -q {{ profile }} -p bench-expansion' \
        --prepare 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features parsyng && cargo clean -q {{ profile }} -p bench-expansion' \
        -n syn 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features syn' \
        -n unsynn 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features unsynn' \
        -n moxy 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features moxy' \
        -n parsyng 'cargo build -q {{ profile }} -p bench-expansion --no-default-features --features parsyng'
