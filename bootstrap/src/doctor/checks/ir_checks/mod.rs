//! IR-related health check implementations.
//!
//! These modules are re-exported from the parent `checks` module
//! so that existing `checks::check_*` paths continue to work.
//!
//! Every directory-scanning audit here shares one corpus contract — walker,
//! candidate/tracked reach measure, and exit-code map — from [`corpus`]
//! (ADR 28.7.26e). `make check-ir-audits` emits one IR corpus and runs the
//! whole family over it.

pub(crate) mod corpus;

pub(crate) mod check_declares;
pub(crate) mod check_indirect_buffers;
#[cfg(feature = "codegen")]
pub(crate) mod check_link_collisions;
pub(crate) mod check_link_health;
pub(crate) mod check_merge_truncation;
pub(crate) mod check_null_calls;
pub(crate) mod check_self_compile_readiness;
// `pub`: the bin crate's codegen regression test (ADR 3.7.26d AC2) audits
// freshly emitted IR via `tungsten_bootstrap::doctor::checks::check_sret_stores`.
pub mod check_sret_stores;
pub(crate) mod check_wrapper_self_calls;
pub(crate) mod textparse;

pub(crate) use corpus::collect_ll_files;
