//! Shared merge planner for branch/arm result unification (ADR 2.7.26b T2).
//!
//! `compile_if`, `sums/case.rs`, and `adt_match.rs` each used to unify their
//! merge-phi result types with a *different* rule (source-type inference,
//! size-max, first-reachable-arm) — and one divergent rule caused the 1.7.26e
//! §6.6 phi-poisoning miscompile. All three now route through [`plan_merge`],
//! which embodies ONE rule:
//!
//! * **Unreachable / musttail-terminated arms are dropped entirely** — they
//!   are not phi predecessors and contribute no incoming value. (Feeding a
//!   `const_zero` placeholder as a merge input was itself the smell that let
//!   §6.6 poison the phi.) The caller terminates such an arm's dead block
//!   with `unreachable` instead of a branch to the merge.
//! * **All reachable value-producing arms must agree** on the result type —
//!   the caller-supplied expected type when given (`compile_if`'s source-type
//!   inference, passed as a production input, not a debug assertion), else
//!   the arms' common type. Disagreement is a hard [`CodeGenError`], never a
//!   silent first-wins/size-max pick (an order-dependent pick would mask a
//!   lowering bug where two reachable arms disagree).
//! * **The all-unreachable case is explicit**: `result_type` is `None` and no
//!   merge value (no phi) is emitted — the expression's result is uninhabited.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use crate::types::TypeLowering;
use inkwell::basic_block::BasicBlock;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;
use tungsten_core::types::Type;

/// One compiled branch/arm, before merging.
#[derive(Debug)]
pub(crate) struct MergeArm<'ctx> {
    pub value: BasicValueEnum<'ctx>,
    /// The block the arm's control flow ends in.
    pub end_bb: BasicBlock<'ctx>,
    /// `false` when the arm terminated control flow itself (musttail self-tail
    /// edge): its `value` is only the epilogue's typed dummy in a dead block.
    pub reachable: bool,
    /// The arm's Tungsten result type, when the site had it in hand. Populated
    /// best-effort; consumed ONLY on the disagree path to self-decode the error
    /// (name the shared type + attribute each layout to its lowering route —
    /// ADR 12.7.26c P2/D2). `None` degrades to the un-enriched message.
    pub source_ty: Option<Type>,
}

/// A complete merge plan: the phi's incoming list and its agreed type.
#[derive(Debug)]
pub(crate) struct MergePlan<'ctx> {
    /// Reachable `(value, block)` edges — the ONLY phi predecessors.
    pub incoming: Vec<(BasicValueEnum<'ctx>, BasicBlock<'ctx>)>,
    /// The agreed result type; `None` when every arm terminated (no merge
    /// value is emitted).
    pub result_type: Option<BasicTypeEnum<'ctx>>,
}

/// Plan a merge over compiled arms (see module docs for the rule).
///
/// `expected` is the caller's authoritative result type when it has one
/// (`compile_if`'s source-inferred type); reachable arms must match it
/// exactly — the caller casts each arm before planning, and `cast_to_type`'s
/// shrinking-aggregate guard (T1) makes a poisoned cast a hard error.
/// `diag`, when supplied, is the codegen type context used ONLY to self-decode
/// a disagreement (ADR 12.7.26c P2/D1): it is untouched on the success path.
pub(crate) fn plan_merge<'ctx>(
    arms: &[MergeArm<'ctx>],
    expected: Option<BasicTypeEnum<'ctx>>,
    site: &str,
    mut diag: Option<&mut TypeLowering<'ctx>>,
) -> Result<MergePlan<'ctx>, CodeGenError> {
    let reachable: Vec<&MergeArm<'ctx>> = arms.iter().filter(|a| a.reachable).collect();

    if reachable.is_empty() {
        return Ok(MergePlan {
            incoming: Vec::new(),
            result_type: None,
        });
    }

    let canonical = expected.unwrap_or_else(|| reachable[0].value.get_type());
    for arm in &reachable {
        let ty = arm.value.get_type();
        if ty != canonical {
            return Err(build_merge_error(
                site,
                canonical,
                reachable[0],
                arm,
                diag.as_deref_mut(),
            ));
        }
    }

    Ok(MergePlan {
        incoming: reachable.into_iter().map(|a| (a.value, a.end_bb)).collect(),
        result_type: Some(canonical),
    })
}

/// Build the hard merge-disagreement error, self-decoding it via the lowering
/// routes when `diag` and both arms' source types are available (ADR 12.7.26c
/// P2). The base message is unchanged from ADR 2.7.26b T2; the enrichment is
/// appended and best-effort.
fn build_merge_error<'ctx>(
    site: &str,
    canonical: BasicTypeEnum<'ctx>,
    first_arm: &MergeArm<'ctx>,
    found_arm: &MergeArm<'ctx>,
    diag: Option<&mut TypeLowering<'ctx>>,
) -> CodeGenError {
    // `found` is always the disagreeing arm's own LLVM type (the loop compared
    // it against `canonical`); derive it rather than threading a 6th param.
    let found = found_arm.value.get_type();
    let base = format!(
        "merge arms disagree on result type in {site}: expected {expected_ty}, \
         found {found_ty} — two reachable arms lowered to different types, \
         which indicates a frontend/lowering bug (ADR 2.7.26b T2; a silent \
         first-wins pick here is the 1.7.26e §6.6 miscompile class)",
        expected_ty = canonical.print_to_string(),
        found_ty = found.print_to_string(),
    );
    let enriched = diag.and_then(|d| {
        d.diagnose_layout_divergence(
            first_arm.source_ty.as_ref(),
            canonical,
            found_arm.source_ty.as_ref(),
            found,
        )
    });
    match enriched {
        Some(block) => CodeGenError::TypeError(format!("{base}\n{block}")),
        None => CodeGenError::TypeError(base),
    }
}

impl<'ctx> CodeGen<'ctx> {
    /// Terminate a compiled arm's end block: a branch to the merge for a
    /// reachable arm; `unreachable` for a terminated (musttail) arm — its dead
    /// block must NOT become a merge predecessor (ADR 2.7.26b T2).
    pub(crate) fn terminate_merge_arm(
        &mut self,
        arm: &MergeArm<'ctx>,
        merge_bb: BasicBlock<'ctx>,
    ) -> Result<(), CodeGenError> {
        self.builder.position_at_end(arm.end_bb);
        if arm.reachable {
            self.builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        } else {
            self.builder
                .build_unreachable()
                .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        }
        Ok(())
    }

    /// Build the merge block from a plan: a phi over the reachable edges only,
    /// or — when every arm terminated — NO phi and a typed placeholder value
    /// (the merge block is dead; downstream code appended to it can never
    /// execute). `dead_placeholder_ty` types that placeholder.
    pub(crate) fn build_planned_merge(
        &mut self,
        merge_bb: BasicBlock<'ctx>,
        plan: &MergePlan<'ctx>,
        dead_placeholder_ty: BasicTypeEnum<'ctx>,
        name: &str,
    ) -> Result<BasicValueEnum<'ctx>, CodeGenError> {
        self.builder.position_at_end(merge_bb);
        let Some(ty) = plan.result_type else {
            return Ok(dead_placeholder_ty.const_zero());
        };
        let phi = self
            .builder
            .build_phi(ty, name)
            .map_err(|e| CodeGenError::LlvmError(e.to_string()))?;
        for (val, bb) in &plan.incoming {
            phi.add_incoming(&[(val, *bb)]);
        }
        Ok(phi.as_basic_value())
    }

    /// Name of the function currently being compiled (for merge-error context).
    pub(crate) fn current_fn_name(&self) -> String {
        self.compilation
            .current_fn
            .map(|f| f.get_name().to_string_lossy().into_owned())
            .unwrap_or_else(|| "<unknown function>".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CodegenConstructor;
    use inkwell::context::Context;
    use std::collections::HashMap;

    fn two_blocks(context: &Context) -> (BasicBlock<'_>, BasicBlock<'_>) {
        let module = context.create_module("merge_test");
        let fn_ty = context.void_type().fn_type(&[], false);
        let f = module.add_function("f", fn_ty, None);
        (
            context.append_basic_block(f, "a"),
            context.append_basic_block(f, "b"),
        )
    }

    /// A minimal arm with no source type (the pre-12.7.26c shape).
    fn arm<'ctx>(
        value: BasicValueEnum<'ctx>,
        end_bb: BasicBlock<'ctx>,
        reachable: bool,
    ) -> MergeArm<'ctx> {
        MergeArm {
            value,
            end_bb,
            reachable,
            source_ty: None,
        }
    }

    /// AC T2(a): the terminated (musttail) arm — FIRST in arm order, the §6.6
    /// trigger — is absent from the incoming list; the type comes from the
    /// reachable arm, not the dummy.
    #[test]
    fn terminated_first_arm_is_dropped_and_type_comes_from_reachable_arm() {
        let context = Context::create();
        let (dead_bb, live_bb) = two_blocks(&context);

        let dummy = context.bool_type().const_zero(); // the epilogue's i1 dummy
        let ptr_ty = context.ptr_type(inkwell::AddressSpace::default());
        let real_ty = context.struct_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let real = real_ty.const_zero();

        let arms = [
            arm(dummy.into(), dead_bb, false),
            arm(real.into(), live_bb, true),
        ];
        let plan = plan_merge(&arms, None, "test", None).unwrap();

        assert_eq!(plan.incoming.len(), 1, "terminated arm must be absent");
        assert_eq!(plan.incoming[0].1, live_bb);
        assert_eq!(
            plan.result_type.unwrap(),
            real_ty.into(),
            "phi type must come from the reachable arm, not the i1 dummy"
        );
    }

    /// AC T2(b): two REACHABLE arms with disagreeing result types are a hard
    /// error — not a silent first-wins (or size-max) pick.
    #[test]
    fn reachable_arms_disagreeing_is_hard_error() {
        let context = Context::create();
        let (a_bb, b_bb) = two_blocks(&context);

        let arms = [
            arm(context.i64_type().const_zero().into(), a_bb, true),
            arm(context.bool_type().const_zero().into(), b_bb, true),
        ];
        let err = plan_merge(&arms, None, "test_site", None).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("disagree"), "{msg}");
        assert!(msg.contains("test_site"), "{msg}");
    }

    /// AC T2(c): all arms terminated → no merge value (no phi type).
    #[test]
    fn all_unreachable_emits_no_merge_value() {
        let context = Context::create();
        let (a_bb, b_bb) = two_blocks(&context);

        let arms = [
            arm(context.bool_type().const_zero().into(), a_bb, false),
            arm(context.bool_type().const_zero().into(), b_bb, false),
        ];
        let plan = plan_merge(&arms, None, "test", None).unwrap();
        assert!(plan.incoming.is_empty());
        assert!(
            plan.result_type.is_none(),
            "no merge value for uninhabited result"
        );
    }

    /// The expected-type input (compile_if's source inference) is authoritative
    /// IN PRODUCTION: a reachable arm that disagrees with it is a hard error.
    #[test]
    fn expected_type_is_enforced_not_debug_only() {
        let context = Context::create();
        let (a_bb, _) = two_blocks(&context);

        let arms = [arm(context.bool_type().const_zero().into(), a_bb, true)];
        let expected: BasicTypeEnum = context.i64_type().into();
        let err = plan_merge(&arms, Some(expected), "if", None).unwrap_err();
        assert!(format!("{err}").contains("disagree"));

        // And agreement passes, keeping the expected type.
        let arms_ok = [arm(context.i64_type().const_zero().into(), a_bb, true)];
        let plan = plan_merge(&arms_ok, Some(expected), "if", None).unwrap();
        assert_eq!(plan.result_type.unwrap(), expected);
    }

    /// AC (P2): with `source_ty` set and a `TypeLowering` diag context, a
    /// disagreement self-decodes — the message names the shared Tungsten type
    /// (recognizing the `TyVar` vs structural-`Sum` spellings as one type, the
    /// D2 rule and the 16d2f4f1 shape) and attributes each layout to a route.
    #[test]
    fn disagreement_self_decodes_type_name_and_routes() {
        let context = Context::create();
        let (a_bb, b_bb) = two_blocks(&context);

        // Verdict = Pass | Fail(String): the two-ctor ADT whose named and
        // structural spellings must lower alike.
        let mut lowering = crate::types::TypeLowering::new(&context);
        let mut adts = HashMap::new();
        adts.insert(
            "Verdict".to_string(),
            (
                vec![],
                vec![
                    CodegenConstructor {
                        name: "Pass".to_string(),
                        fields: vec![],
                        index: 0,
                    },
                    CodegenConstructor {
                        name: "Fail".to_string(),
                        fields: vec![Type::String],
                        index: 1,
                    },
                ],
            ),
        );
        lowering.register_adt_types(adts);

        // Arm 1 carries the real blob layout for Verdict; arm 2 carries a
        // hand-built typed payload (the historical W4 shape no current route
        // produces) — forcing the disagreement the diagnosis decodes.
        let blob = lowering.lower_type(&Type::TyVar("Verdict".to_string()));
        let ptr_ty = context.ptr_type(inkwell::AddressSpace::default());
        let typed = context
            .struct_type(&[context.i32_type().into(), ptr_ty.into()], false)
            .const_zero();

        let arms = [
            MergeArm {
                value: blob.const_zero(),
                end_bb: a_bb,
                reachable: true,
                source_ty: Some(Type::TyVar("Verdict".to_string())),
            },
            MergeArm {
                value: typed.into(),
                end_bb: b_bb,
                reachable: true,
                source_ty: Some(Type::sum(Type::Unit, Type::String)),
            },
        ];
        let err = plan_merge(&arms, None, "sum case", Some(&mut lowering)).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("both arms are `Verdict`"), "{msg}");
        assert!(msg.contains("route"), "{msg}");
        assert!(msg.contains("lowering-consistency"), "{msg}");
    }
}
