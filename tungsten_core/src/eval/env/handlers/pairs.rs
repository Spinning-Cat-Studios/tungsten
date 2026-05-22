//! Step handlers for pairs and projections.

use crate::terms::Term;

use crate::eval::env::step_with_env;
use crate::eval::env::EvalEnv;
use crate::eval::StepResult;
/// Step Pair with environment: evaluate both components to values.
pub(in crate::eval::env) fn step_pair_env(t1: &Term, t2: &Term, env: &EvalEnv) -> StepResult {
    if !t1.is_value() {
        match step_with_env(t1, env) {
            StepResult::Stepped(t1_new) => {
                return StepResult::Stepped(Term::pair(t1_new, t2.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    if !t2.is_value() {
        match step_with_env(t2, env) {
            StepResult::Stepped(t2_new) => {
                return StepResult::Stepped(Term::pair(t1.clone(), t2_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    StepResult::Value
}

/// Step Fst with environment: evaluate argument, then project first component.
pub(in crate::eval::env) fn step_fst_env(t: &Term, env: &EvalEnv) -> StepResult {
    if !t.is_value() {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => return StepResult::Stepped(Term::fst(t_new)),
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::Pair(v1, _) => StepResult::Stepped(v1.as_ref().clone()),
        _ => StepResult::Stuck,
    }
}

/// Step Snd with environment: evaluate argument, then project second component.
pub(in crate::eval::env) fn step_snd_env(t: &Term, env: &EvalEnv) -> StepResult {
    if !t.is_value() {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => return StepResult::Stepped(Term::snd(t_new)),
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::Pair(_, v2) => StepResult::Stepped(v2.as_ref().clone()),
        _ => StepResult::Stuck,
    }
}
