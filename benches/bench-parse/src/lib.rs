//! Invokes `parse_bench!` (which emits nothing) on the common subset of the
//! tokio fixtures, so building this crate measures parsing speed on the
//! compiler's real `proc_macro` (see `gen.sh`).

include!("input.rs");
