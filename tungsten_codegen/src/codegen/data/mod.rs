//! Data type compilation
//!
//! Compilation of Tungsten data types to LLVM IR:
//! - `primitives`: Basic types (bool, nat, unit)
//! - `products`: Product types (pairs, tuples)
//! - `sums`: Sum types and case expressions
//! - `adt`: Algebraic data types (flat enum representation)
//! - `mu_types`: Recursive type helpers (fold/unfold)
//! - `ops`: Data type operations (bool, nat, string, ref)

pub(crate) mod adt;
pub(crate) mod mu_types;
pub(crate) mod ops;
pub(crate) mod primitives;
pub(crate) mod products;
pub(crate) mod sums;
