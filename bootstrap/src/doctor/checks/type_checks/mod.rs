//! Type-system health check implementations.
//!
//! These modules are re-exported from the parent `checks` module
//! so that existing `checks::check_*` paths continue to work.

pub(crate) mod check_comparable;
pub(crate) mod check_constructor_stubs;
pub(crate) mod check_encoding_depth;
pub(crate) mod check_forall_resolution;
/// Route-lowering consistency check (ADR 12.7.26c) — instantiates a
/// `TypeLowering`, so it is only present under the `codegen` feature.
#[cfg(feature = "codegen")]
pub(crate) mod check_lowering_consistency;
pub(crate) mod check_normalization;
pub(crate) mod check_phase_invariants;
pub(crate) mod check_positivity;
pub(crate) mod check_signature_collection;
pub(crate) mod check_stubs;
pub(crate) mod check_type_sizes;
pub(crate) mod check_vacuous_mu;
