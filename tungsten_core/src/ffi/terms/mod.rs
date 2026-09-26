//! FFI term operations.
//!
//! - `nodes`: handle-children arena representation + import/materialize (ADR 2.7.26a §4)
//! - `core`: Core structural term constructors (lambda, app, var, if, fix, etc.)
//! - `core_data`: Data constructor FFI functions (zero, succ, pair, inl, inr, etc.)
//! - `ext`: Extended term constructors (arithmetic, string ops, ADT, etc.)
//! - `primitives`: Primitive term constructors (nat_lit, bool_lit, string_lit, etc.)
//! - `strings`: String term constructors (concat, eq, len, substring)
//!
//! Constructors are O(1) node pushes: children are referenced by handle and
//! embedded types stay type-node handles — never cloned (ADR 2.7.26a §4).

pub(super) mod core;
pub(super) mod core_data;
pub(super) mod ext;
pub(crate) mod nodes;
// `pub(crate)`: `terms/int_tests.rs` drives the five `Int` constructors.
pub(crate) mod primitives;
pub(super) mod strings;

pub(super) use crate::ffi::{valid_terms, valid_types};
