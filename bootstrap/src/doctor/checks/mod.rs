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
pub(crate) use type_checks::check_constructor_stubs;
pub(crate) use type_checks::check_encoding_depth;
pub(crate) use type_checks::check_forall_resolution;
pub(crate) use type_checks::check_normalization;
pub(crate) use type_checks::check_phase_a5;
pub(crate) use type_checks::check_phase_invariants;
pub(crate) use type_checks::check_stubs;
pub(crate) use type_checks::check_type_sizes;

// Re-export IR checks
#[cfg(feature = "codegen")]
pub(crate) use ir_checks::check_link_collisions;
pub(crate) use ir_checks::check_link_health;
pub(crate) use ir_checks::check_null_calls;
pub(crate) use ir_checks::check_self_compile_readiness;

// Multi-file check modules (already subdirectories)
pub mod check_constructor_counts;
pub(crate) mod check_declares;
pub(crate) mod check_fold_consistency;
pub(crate) mod check_ir_layout;
pub mod check_nested_patterns;
pub(crate) mod check_reexport_completeness;
