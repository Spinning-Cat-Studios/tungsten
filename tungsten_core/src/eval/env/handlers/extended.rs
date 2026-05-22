//! Step handlers for equality transport (subst) and ADT match.

use crate::terms::Term;
use crate::types::Type;

use crate::eval::env::step_with_env;
use crate::eval::env::EvalEnv;
use crate::eval::StepResult;
/// Step subst (equality transport) with environment.
pub(in crate::eval::env) fn step_subst_env(
    ty: &Type,
    motive: &Type,
    eq_proof: &Term,
    proof: &Term,
    env: &EvalEnv,
) -> StepResult {
    if !eq_proof.is_value() {
        match step_with_env(eq_proof, env) {
            StepResult::Stepped(eq_new) => {
                return StepResult::Stepped(Term::subst(
                    ty.clone(),
                    motive.clone(),
                    eq_new,
                    proof.clone(),
                ));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match eq_proof {
        Term::Refl(_, _) => StepResult::Stepped(proof.clone()),
        _ => StepResult::Stuck,
    }
}

/// Step ADT match with environment.
pub(in crate::eval::env) fn step_adt_match_env(
    scrut: &Term,
    arms: &[(usize, String, Box<Term>)],
    env: &EvalEnv,
) -> StepResult {
    if !scrut.is_value() {
        match step_with_env(scrut, env) {
            StepResult::Stepped(scrut_new) => {
                return StepResult::Stepped(Term::adt_match(scrut_new, arms.to_vec()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    match scrut {
        Term::AdtConstruct(_, tag, payload) => {
            for (arm_idx, var, body) in arms {
                if arm_idx == tag {
                    let result = body.substitute(var, payload);
                    return StepResult::Stepped(result);
                }
            }
            StepResult::Stuck
        }
        _ => StepResult::Stuck,
    }
}
