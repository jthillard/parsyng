//! Placeholder extension trait for [`Literal`](crate::proc_macro::Literal),
//! reserved for future proc-macro-specific token helpers.

/// Extension trait for [`Literal`](crate::proc_macro::Literal). Currently a
/// no-op placeholder.
pub trait LiteralExt {
    /// No-op placeholder.
    fn foo(&mut self);
}

impl LiteralExt for crate::proc_macro::Literal {
    fn foo(&mut self) {}
}
