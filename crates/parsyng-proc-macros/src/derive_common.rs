//! Helpers shared by the `Parse` and `ToTokens` derives.

use parsyng_core::ast::item::{enum_item::EnumVariantFields, r#struct::StructFields};
use parsyng_core::proc_macro::{Ident, Span};

/// The shape of a struct's or enum variant's fields.
pub enum Fields {
    /// `{ a: A, b: B }`, with the field names.
    Named(Vec<Ident>),
    /// `(A, B)`, with the field count.
    Unnamed(usize),
    /// No fields.
    Unit,
}

impl Fields {
    /// The shape of a struct's fields.
    pub fn of_struct(fields: &StructFields) -> Self {
        match fields {
            StructFields::Named(fields) => {
                Self::Named(fields.iter().map(|field| field.ident().clone()).collect())
            }
            StructFields::Unnamed(fields) => Self::Unnamed(fields.iter().count()),
            StructFields::Unit => Self::Unit,
        }
    }

    /// The shape of an enum variant's fields.
    pub fn of_variant(fields: &EnumVariantFields) -> Self {
        match fields {
            EnumVariantFields::Named(fields) => {
                Self::Named(fields.iter().map(|field| field.ident().clone()).collect())
            }
            EnumVariantFields::Unnamed(fields) => Self::Unnamed(fields.iter().count()),
            EnumVariantFields::Unit => Self::Unit,
        }
    }
}

/// A fresh binding name for the `index`-th field, used when destructuring
/// enum variants (so that field names can't shadow the generated code's own
/// variables).
pub fn binding(index: usize) -> Ident {
    Ident::new(&format!("__parsyng_field_{index}"), Span::call_site())
}
