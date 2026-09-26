//! `$direct_mt` parameter-slot layout — the single source of truth for how a
//! musttail-eligible entry's source parameters map onto LLVM argument slots
//! (ADRs 18.5.26a, 1.7.26a, 1.7.26e).
//!
//! Slot order (ADR 1.7.26e §2.1, extends 1.7.26a's "sret first, then env"):
//!
//! ```text
//!   [sret out-ptr]?   (if the return is a by-value aggregate)
//!   indirect buffers  (one ptr per non-flattenable struct param, source order)
//!   env ptr
//!   after-env args    (per non-indirect param: decomposed scalars | passthrough)
//! ```
//!
//! The indirect buffers lead (before `env`) so the whole leading run is `ptr`;
//! flattenable-struct scalars and plain passthrough args stay after `env`,
//! preserving the pre-1.7.26e layout exactly when there are zero indirect params.

/// How one source parameter is lowered into the `$direct_mt` signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamLowering {
    /// Passed through unchanged — scalar, `ptr`, or recursive-ADT (`Mu`).
    Passthrough,
    /// Flattenable struct decomposed into `n` scalar field args (18.5.26a).
    Decompose(u32),
    /// Non-flattenable struct passed by a caller-owned buffer `ptr` (1.7.26e).
    Indirect,
}

impl ParamLowering {
    /// Project to the report-layer [`ParamAbiKind`](crate::codegen::musttail_report::ParamAbiKind)
    /// surfaced by `info codegen indirect-abi`.
    pub(crate) fn to_abi_kind(self) -> crate::codegen::musttail_report::ParamAbiKind {
        use crate::codegen::musttail_report::ParamAbiKind;
        match self {
            ParamLowering::Passthrough => ParamAbiKind::ByValue,
            ParamLowering::Decompose(n) => ParamAbiKind::Decomposed(n),
            ParamLowering::Indirect => ParamAbiKind::Indirect,
        }
    }
}

/// Resolved argument-slot position(s) for one source parameter, given the
/// concrete `$direct_mt` layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamSlot {
    /// Indirect buffer `ptr` at this LLVM arg index (before `env`).
    Indirect { buf_index: u32 },
    /// `field_count` scalar args starting at this LLVM arg index (after `env`).
    Decompose { start: u32, field_count: u32 },
    /// A single passthrough value at this LLVM arg index (after `env`).
    Passthrough { index: u32 },
}

/// The resolved slot plan for a `$direct_mt` entry: one [`ParamSlot`] per source
/// parameter (source order) plus the `env` argument index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MtSlotPlan {
    pub slots: Vec<ParamSlot>,
    pub env_index: u32,
}

/// Number of leading `ptr` slots before `env`: the optional sret out-ptr plus
/// one buffer per [`ParamLowering::Indirect`] parameter.
fn leading_ptr_count(lowerings: &[ParamLowering], sret: bool) -> u32 {
    let indirect = lowerings
        .iter()
        .filter(|l| matches!(l, ParamLowering::Indirect))
        .count() as u32;
    u32::from(sret) + indirect
}

/// Compute the argument-slot plan for a `$direct_mt` entry from the per-param
/// lowerings and whether the return is sret.
///
/// Indirect buffers occupy `[sret_off .. sret_off + n_indirect)`; `env` sits at
/// `sret_off + n_indirect`; decomposed/passthrough args follow `env` in source
/// order. This is the ONE place slot indices are computed — declaration,
/// callee binding, the `$direct` shim, and the self-tail edge all consult it so
/// they cannot drift (ADR 1.7.26e R6/R8, env-ordering).
pub(crate) fn plan_mt_slots(lowerings: &[ParamLowering], sret: bool) -> MtSlotPlan {
    let sret_off = u32::from(sret);
    let env_index = leading_ptr_count(lowerings, sret);

    let mut indirect_cursor = sret_off;
    let mut after_env_cursor = env_index + 1;
    let mut slots = Vec::with_capacity(lowerings.len());

    for lowering in lowerings {
        let slot = match *lowering {
            ParamLowering::Indirect => {
                let buf_index = indirect_cursor;
                indirect_cursor += 1;
                ParamSlot::Indirect { buf_index }
            }
            ParamLowering::Decompose(field_count) => {
                let start = after_env_cursor;
                after_env_cursor += field_count;
                ParamSlot::Decompose { start, field_count }
            }
            ParamLowering::Passthrough => {
                let index = after_env_cursor;
                after_env_cursor += 1;
                ParamSlot::Passthrough { index }
            }
        };
        slots.push(slot);
    }

    MtSlotPlan { slots, env_index }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_special_params_matches_legacy_layout() {
        // All passthrough, no sret: env at 0, args after env — the pre-1.7.26e shape.
        let plan = plan_mt_slots(
            &[ParamLowering::Passthrough, ParamLowering::Passthrough],
            false,
        );
        assert_eq!(plan.env_index, 0);
        assert_eq!(
            plan.slots,
            vec![
                ParamSlot::Passthrough { index: 1 },
                ParamSlot::Passthrough { index: 2 },
            ]
        );
    }

    #[test]
    fn sret_only_shifts_env_by_one() {
        // Class-R: sret out-ptr at 0, env at 1, passthrough args after.
        let plan = plan_mt_slots(&[ParamLowering::Passthrough], true);
        assert_eq!(plan.env_index, 1);
        assert_eq!(plan.slots, vec![ParamSlot::Passthrough { index: 2 }]);
    }

    #[test]
    fn decompose_after_env() {
        // Flattenable struct → scalars after env (18.5.26a), no sret.
        let plan = plan_mt_slots(
            &[ParamLowering::Decompose(3), ParamLowering::Passthrough],
            false,
        );
        assert_eq!(plan.env_index, 0);
        assert_eq!(
            plan.slots,
            vec![
                ParamSlot::Decompose {
                    start: 1,
                    field_count: 3
                },
                ParamSlot::Passthrough { index: 4 },
            ]
        );
    }

    #[test]
    fn indirect_leads_before_env_sret() {
        // Class-P collect_type_names shape: sret + indirect ctx + env + passthrough items.
        // Layout: [0]=sret, [1]=ctx_buf, [2]=env, [3]=items.
        let plan = plan_mt_slots(&[ParamLowering::Indirect, ParamLowering::Passthrough], true);
        assert_eq!(plan.env_index, 2);
        assert_eq!(
            plan.slots,
            vec![
                ParamSlot::Indirect { buf_index: 1 },
                ParamSlot::Passthrough { index: 3 },
            ]
        );
    }

    #[test]
    fn indirect_no_sret_leads_at_zero() {
        // No-sret Class-P (R4): scalar/void return, indirect run leads at 0.
        let plan = plan_mt_slots(&[ParamLowering::Indirect], false);
        assert_eq!(plan.env_index, 1);
        assert_eq!(plan.slots, vec![ParamSlot::Indirect { buf_index: 0 }]);
    }

    #[test]
    fn multiple_indirect_in_source_order() {
        let plan = plan_mt_slots(&[ParamLowering::Indirect, ParamLowering::Indirect], true);
        // [0]=sret, [1]=buf a, [2]=buf b, [3]=env.
        assert_eq!(plan.env_index, 3);
        assert_eq!(
            plan.slots,
            vec![
                ParamSlot::Indirect { buf_index: 1 },
                ParamSlot::Indirect { buf_index: 2 },
            ]
        );
    }

    #[test]
    fn mixed_indirect_decompose_passthrough() {
        // sret + indirect + flattenable(2) + passthrough.
        // [0]=sret, [1]=indirect buf, [2]=env, [3..5]=decompose, [5]=passthrough.
        let plan = plan_mt_slots(
            &[
                ParamLowering::Indirect,
                ParamLowering::Decompose(2),
                ParamLowering::Passthrough,
            ],
            true,
        );
        assert_eq!(plan.env_index, 2);
        assert_eq!(
            plan.slots,
            vec![
                ParamSlot::Indirect { buf_index: 1 },
                ParamSlot::Decompose {
                    start: 3,
                    field_count: 2
                },
                ParamSlot::Passthrough { index: 5 },
            ]
        );
    }
}
