//! Data type compilation
//!
//! Compilation of Tungsten data types to LLVM IR:
//! - `primitives`: Basic types (bool, nat, unit)
//! - `products`: Product types (pairs, tuples)
//! - `sums`: Sum types and case expressions
//! - `adt`: Algebraic data types (flat enum representation)
//! - `strings`: String operations
//! - `refs`: Mutable references
//! - `mu_types`: Recursive type helpers (fold/unfold)
//! - `nat_ops`: Natural number arithmetic and comparisons
//! - `bool_ops`: Boolean logic operations

pub(crate) mod adt;
pub(crate) mod bool_ops;
pub(crate) mod mu_types;
pub(crate) mod nat_ops;
pub(crate) mod primitives;
pub(crate) mod products;
pub(crate) mod refs;
pub(crate) mod strings;
pub(crate) mod sums;
