#!/usr/bin/env bash
# Regenerates src/input.rs from the tokio fixtures in parsyng-core's tests.
set -euo pipefail
cd "$(dirname "$0")"
cargo run -q -p parsyng-bench-runtime --example common_subset > src/input.rs
