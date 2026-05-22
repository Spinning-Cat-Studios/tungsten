//! Diff commands — structural comparison of compiler outputs.
//!
//! - `abi`: compare L1 vs L2 ABI layouts (requires codegen)
//! - `l1_l2`: compare L1 vs L2 elaboration/check output

#[cfg(feature = "codegen")]
pub(crate) mod abi;
pub(crate) mod l1_l2;
