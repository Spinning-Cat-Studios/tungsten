//! FFI term operations.
//!
//! - `core`: Core structural term constructors (lambda, app, var, if, fix, etc.)
//! - `core_data`: Data constructor FFI functions (zero, succ, pair, inl, inr, etc.)
//! - `ext`: Extended term constructors (arithmetic, string ops, ADT, etc.)
//! - `primitives`: Primitive term constructors (nat_lit, bool_lit, string_lit, etc.)

pub(super) mod core;
pub(super) mod core_data;
pub(super) mod ext;
pub(super) mod primitives;
