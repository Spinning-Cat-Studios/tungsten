//! Diff commands — structural comparison of compiler outputs.
//!
//! - `abi`: compare bootstrap vs self-host ABI layouts (requires codegen)
//! - `bootstrap_selfhost`: compare bootstrap vs self-host elaboration/check output
//! - `selfhost_core`: compare one definition's Core term across both compilers
//! - `exec`: differential evaluator-vs-native execution (ADR 3.7.26d)
//! - `cache`: differential cold-vs-warm cache parity (ADR 4.7.26d)

#[cfg(feature = "codegen")]
pub(crate) mod abi;
pub(crate) mod bootstrap_selfhost;
// `selfhost_core` compares one definition's Core TERM across the two
// compilers — the question the outcome-comparing siblings above cannot ask
// (ADR 19.8.26d retrospective). Needs no codegen: both sides only elaborate.
pub(crate) mod selfhost_core;
// `cache` runs on the evaluator (`run`/`test`) and needs no codegen — always
// compiled and available. It reuses `exec`'s subprocess runner, so `exec` is
// compiled unconditionally too (its own CLI subcommand stays codegen-gated).
pub(crate) mod cache;
#[cfg_attr(not(feature = "codegen"), allow(dead_code))]
pub(crate) mod exec;
