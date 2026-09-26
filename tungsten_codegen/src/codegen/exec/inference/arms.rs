//! Dead-arm-aware result-type inference for multi-arm control flow
//! (ADR 3.7.26a).
//!
//! Type-level analogue of the shared merge planner's reachable-arm exclusion
//! (ADR 2.7.26b T2): an arm whose type is ⊥ (`Type::Void`) is control-flow
//! terminated (early `return`) and must not contribute its placeholder type
//! to the enclosing expression's result type. With ≥1 live arm the live
//! arms' type wins; with zero live arms the whole expression is itself ⊥
//! (unreachable-after-terminators). A terminated arm's `return` value is
//! validated against the enclosing function's return type at the emission
//! site (`compile_return`), independently of this unification.

use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use std::collections::HashMap;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// Unify per-arm inferred result types, excluding dead (⊥-typed) arms.
///
/// - The first live (non-`Void`) `Ok` arm wins — matching the simplified
///   inferencer's pre-existing first-arm-wins convention.
/// - All inferable arms dead → `Void` (the expression never produces a value).
/// - Arms whose inference fails are skipped (the simplified inferencer cannot
///   type every arm shape); if no arm infers at all, the first error is
///   propagated.
pub(super) fn unify_arm_result_types(
    arms: impl IntoIterator<Item = Result<Type, CodeGenError>>,
) -> Result<Type, CodeGenError> {
    let mut saw_dead_arm = false;
    let mut first_err: Option<CodeGenError> = None;
    for arm in arms {
        match arm {
            Ok(Type::Void) => saw_dead_arm = true,
            Ok(ty) => return Ok(ty),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    if saw_dead_arm {
        Ok(Type::Void)
    } else {
        Err(first_err.unwrap_or_else(|| {
            CodeGenError::TypeError("arm-type unification over zero arms".to_string())
        }))
    }
}

impl CodeGen<'_> {
    /// Infer the type of an If expression (dead-arm aware).
    pub(super) fn infer_if_type(
        &self,
        then_: &Term,
        else_: &Term,
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        unify_arm_result_types([
            self.infer_term_type_with_ctx(then_, local_ctx),
            self.infer_term_type_with_ctx(else_, local_ctx),
        ])
    }

    /// Infer the type of a Case expression (dead-arm aware).
    pub(super) fn infer_case_type(
        &self,
        scrut: &Term,
        left: (&str, &Term),
        right: (&str, &Term),
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        if let Ok(scrut_ty) = self.infer_term_type_with_ctx(scrut, local_ctx) {
            if let Type::Sum(ty_l, ty_r) = scrut_ty {
                let mut left_ctx = local_ctx.clone();
                left_ctx.insert(left.0.to_owned(), ty_l.as_ref().clone());
                let mut right_ctx = local_ctx.clone();
                right_ctx.insert(right.0.to_owned(), ty_r.as_ref().clone());
                return unify_arm_result_types([
                    self.infer_term_type_with_ctx(left.1, &left_ctx),
                    self.infer_term_type_with_ctx(right.1, &right_ctx),
                ]);
            }
        }
        unify_arm_result_types([
            self.infer_term_type_with_ctx(left.1, local_ctx),
            self.infer_term_type_with_ctx(right.1, local_ctx),
        ])
    }

    /// Infer the type of an `AdtMatch` expression (dead-arm aware).
    ///
    /// Each arm's variable is bound to its own variant's payload type (the
    /// arm tuple carries the variant index) — not unconditionally the first
    /// variant's, as the pre-3.7.26a single-arm inference did.
    pub(super) fn infer_adt_match_type(
        &self,
        scrutinee: &Term,
        arms: &[(usize, String, Box<Term>)],
        local_ctx: &HashMap<String, Type>,
    ) -> Result<Type, CodeGenError> {
        if arms.is_empty() {
            return Err(CodeGenError::TypeError("AdtMatch with no arms".to_string()));
        }
        let variants = match self.infer_term_type_with_ctx(scrutinee, local_ctx)? {
            Type::Adt(_, _, variants) => Some(variants),
            _ => None,
        };
        let candidates = arms.iter().map(|(idx, var, body)| {
            match variants.as_ref().and_then(|vs| vs.get(*idx)) {
                Some((_, payload_ty)) => {
                    let mut arm_ctx = local_ctx.clone();
                    arm_ctx.insert(var.clone(), payload_ty.clone());
                    self.infer_term_type_with_ctx(body, &arm_ctx)
                }
                None => self.infer_term_type_with_ctx(body, local_ctx),
            }
        });
        unify_arm_result_types(candidates)
    }
}
