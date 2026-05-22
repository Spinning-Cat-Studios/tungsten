//! IR-related health check implementations.
//!
//! These modules are re-exported from the parent `checks` module
//! so that existing `checks::check_*` paths continue to work.

#[cfg(feature = "codegen")]
pub(crate) mod check_link_collisions;
pub(crate) mod check_link_health;
pub(crate) mod check_null_calls;
pub(crate) mod check_self_compile_readiness;
