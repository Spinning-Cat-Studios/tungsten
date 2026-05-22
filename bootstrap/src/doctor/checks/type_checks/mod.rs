//! Type-system health check implementations.
//!
//! These modules are re-exported from the parent `checks` module
//! so that existing `checks::check_*` paths continue to work.

pub(crate) mod check_constructor_stubs;
pub(crate) mod check_encoding_depth;
pub(crate) mod check_forall_resolution;
pub(crate) mod check_normalization;
pub(crate) mod check_phase_a5;
pub(crate) mod check_phase_invariants;
pub(crate) mod check_stubs;
pub(crate) mod check_type_sizes;
