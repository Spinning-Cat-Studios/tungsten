//! Health check subcommands grouped under `tungsten doctor check`.
//!
//! Each module implements one check command. Standalone type-system checks
//! live in `type_checks/`, IR checks in `ir_checks/`, and multi-file checks
//! remain as top-level subdirectories. Re-exports preserve existing
//! `checks::check_*` paths for consumers.

// Grouped submodules
pub(crate) mod ir_checks;
pub(crate) mod type_checks;

// Re-export type-system checks
pub(crate) use type_checks::check_comparable;
pub(crate) use type_checks::check_constructor_stubs;
pub(crate) use type_checks::check_encoding_depth;
pub(crate) use type_checks::check_forall_resolution;
#[cfg(feature = "codegen")]
pub(crate) use type_checks::check_lowering_consistency;
pub(crate) use type_checks::check_normalization;
pub(crate) use type_checks::check_phase_invariants;
pub(crate) use type_checks::check_positivity;
pub(crate) use type_checks::check_signature_collection;
pub(crate) use type_checks::check_stubs;
pub(crate) use type_checks::check_type_sizes;
pub(crate) use type_checks::check_vacuous_mu;

// Re-export IR checks
// NOTE: `check_indirect_buffers` is text-based (audits emitted `.ll`) and
// works without the codegen feature; `check_link_collisions` requires codegen.
pub(crate) use ir_checks::check_indirect_buffers;
#[cfg(feature = "codegen")]
pub(crate) use ir_checks::check_link_collisions;
// `check_declares` moved under `ir_checks/` in ADR 28.7.26e: living outside the
// family is why 28.7.26e's own first pass missed it when enumerating the
// dir-scanning IR audits, and it was red.
pub(crate) use ir_checks::check_declares;
pub(crate) use ir_checks::check_link_health;
pub(crate) use ir_checks::check_merge_truncation;
pub(crate) use ir_checks::check_null_calls;
pub(crate) use ir_checks::check_self_compile_readiness;
pub use ir_checks::check_sret_stores;
pub(crate) use ir_checks::check_wrapper_self_calls;

// Multi-file check modules (already subdirectories)
pub mod check_constructor_counts;
pub mod check_extern_coverage;
pub mod check_extern_symbols;
pub(crate) mod check_fold_consistency;
pub(crate) mod check_ir_layout;
pub mod check_name_collisions;
pub mod check_nested_patterns;
pub(crate) mod check_reexport_completeness;
pub mod check_selfhost_closed_terms;
pub mod check_selfhost_well_typed_terms;
pub mod check_sorry_sites;
pub mod check_termination;
pub mod check_tool_reachability;
