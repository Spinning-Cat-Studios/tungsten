//! Step handlers for type application, annotation, and recursive type unfold.

use crate::terms::Term;
use crate::types::Type;

use crate::eval::env::step_with_env;
use crate::eval::env::EvalEnv;
use crate::eval::{ComparatorFailure, ComparatorFailureKind, EvalStopped, StepResult};
/// Step TyApp with environment: evaluate, then erase type abstraction.
///
/// Special case (ADR 29.6.26f §T11.2a): `TyApp(Global("__cmp"), ConcreteT)` is the
/// structural-comparator intrinsic. It is resolved by the env's `ComparatorSynth`
/// callback to the synthesized `compare_T` (registering the closure's defs), then
/// stepped to `Global(compare_T)`. Falls through to normal `TyAbs` erasure
/// otherwise.
pub(in crate::eval::env) fn step_tyapp_env(t: &Term, ty: &Type, env: &EvalEnv) -> StepResult {
    if let Term::Global(name) = t {
        if name == crate::eval::COMPARE_INTRINSIC {
            return resolve_comparator_intrinsic(ty, env);
        }
    }
    if !t.is_value() {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => {
                return StepResult::Stepped(Term::ty_app(t_new, ty.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        // Substitute the type argument into the body (rather than merely erasing
        // the abstraction). Type annotations don't affect computed values, but the
        // `__cmp<T>` intrinsic *inspects* its type argument, so the concrete `T`
        // must reach it (ADR 29.6.26f §T11.2a).
        Term::TyAbs(v, body) => {
            let mut subst = std::collections::HashMap::new();
            subst.insert(v.clone(), ty.clone());
            StepResult::Stepped(body.substitute_type_vars(&subst))
        }
        _ => StepResult::Stuck,
    }
}

/// Resolve `__cmp<T>` via the env's comparator synthesis callback. Synthesizes
/// the comparator closure for a concrete `T`, registers its defs, and steps to
/// `Global(compare_T)`.
///
/// A failure is recorded on the env and steps to `Stuck` — the black-hole
/// treatment (ADR 22.7.26a / D2), for the same reason: interior stepping must
/// terminate, but the reporting boundary must not mistake the residual for a
/// value. Before ADR 1.8.26b this returned a bare `Stuck`, the enclosing
/// `assert_eq` never executed, and `tungsten test` reported the test `ok`.
fn resolve_comparator_intrinsic(ty: &Type, env: &EvalEnv) -> StepResult {
    let Some(synth) = env.comparator_synth() else {
        env.record_stop(EvalStopped::Uncomparable(ComparatorFailure::new(
            ty.to_string(),
            ComparatorFailureKind::NoSynthesizer,
        )));
        return StepResult::Stuck;
    };
    match synth(ty) {
        Ok((top_symbol, defs)) => {
            env.register_comparators(defs);
            StepResult::Stepped(Term::Global(top_symbol))
        }
        Err(failure) => {
            env.record_stop(EvalStopped::Uncomparable(failure));
            StepResult::Stuck
        }
    }
}

/// Step Annot with environment: strip annotation, evaluate inner term.
pub(in crate::eval::env) fn step_annot_env(t: &Term, ty: &Type, env: &EvalEnv) -> StepResult {
    if t.is_value() {
        StepResult::Stepped(t.clone())
    } else {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => StepResult::Stepped(Term::annot(t_new, ty.clone())),
            other => other,
        }
    }
}

/// Step Unfold with environment: evaluate argument, then unwrap Fold.
pub(in crate::eval::env) fn step_unfold_env(t: &Term, ty: &Type, env: &EvalEnv) -> StepResult {
    if !t.is_value() {
        match step_with_env(t, env) {
            StepResult::Stepped(t_new) => {
                return StepResult::Stepped(Term::unfold(ty.clone(), t_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }
    match t {
        Term::Fold(_, inner) => StepResult::Stepped(inner.as_ref().clone()),
        // Unfolding a value that carries no `Fold` can never step. It is what
        // an unfold/fold *count* mismatch looks like at runtime (ADR 1.8.26b
        // D2: a nested-μ chain unfolds once per binder while the value was
        // folded once), and leaving it a bare `Stuck` is what made that
        // defect report `ok`.
        _ => super::malformed::malformed_elimination("Unfold", t, env),
    }
}
