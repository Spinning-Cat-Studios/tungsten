//! Expression form elaboration.
//!
//! Each module implements trait methods on `Elaborator` for a specific expression form.

mod builtins;
mod lambda;
mod operators;
mod proofs;
mod records;
mod tuples;
mod type_args;

pub(in crate::elaborate::exprs) use operators::{int_literal_out_of_range, int_literal_value};
