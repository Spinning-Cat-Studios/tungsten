//! Eliminators applied to the wrong value shape (ADR 1.8.26b).
//!
//! `Fst`/`Snd` of a non-pair and `Unfold` of a non-`Fold` are not "the program
//! is incomplete" — unlike an unresolved `Global`, whose definition might yet
//! arrive, such a term can **never** step whatever else is in scope. Leaving
//! them a bare `Stuck` is how two silent defect classes survived: the enclosing
//! assertion never ran, so `tungsten test` reported the test `ok`.
//!
//! Both are representation mismatches, and each names its own:
//!
//! | Eliminator | Mismatch | Instance |
//! |---|---|---|
//! | `Fst`/`Snd` | constructor payload nested one way, projected the other | ADR 1.8.26b D1 |
//! | `Unfold` | more unfolds than the value carries `Fold`s | ADR 1.8.26b D2 |

use crate::eval::env::EvalEnv;
use crate::eval::{EvalStopped, StepResult};
use crate::terms::Term;

/// Record the mismatch on the env and step to `Stuck`.
///
/// The `Stuck` is what interior stepping needs (the loops must terminate); the
/// recorded reason is what the reporting boundary needs, so the residual is
/// never mistaken for a value. Same division as black-hole detection
/// (ADR 22.7.26a / D2).
pub(in crate::eval::env) fn malformed_elimination(
    eliminator: &'static str,
    operand: &Term,
    env: &EvalEnv,
) -> StepResult {
    env.record_stop(EvalStopped::MalformedElimination {
        eliminator,
        head: value_shape(operand).to_string(),
    });
    StepResult::Stuck
}

/// The value's shape, for the diagnostic.
///
/// Names *what was there instead*, which is the fact a reader needs — the whole
/// residual term is what they already could not read.
fn value_shape(term: &Term) -> &'static str {
    match term {
        Term::Zero | Term::Succ(_) | Term::NatLit(_) => "a Nat",
        Term::True | Term::False => "a Bool",
        Term::StringLit(_) => "a String",
        Term::Unit => "Unit",
        Term::Lambda(..) | Term::TyAbs(..) => "a function",
        Term::Pair(..) => "a pair",
        Term::Inl(..) | Term::Inr(..) => "a sum injection",
        Term::Fold(..) => "a folded recursive value",
        Term::AdtConstruct(..) => "an ADT constructor",
        Term::Spanned(inner, _) => value_shape(inner),
        _ => "a value of another shape",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fst_of_a_scalar_is_recorded_not_silently_stuck() {
        let env = EvalEnv::empty();
        assert!(matches!(
            malformed_elimination("Fst", &Term::Zero, &env),
            StepResult::Stuck
        ));
        let Some(EvalStopped::MalformedElimination { eliminator, head }) = env.recorded_stop()
        else {
            panic!("the mismatch must ride the env to the reporting boundary");
        };
        assert_eq!(eliminator, "Fst");
        assert_eq!(head, "a Nat");
    }

    #[test]
    fn the_first_mismatch_wins() {
        // Evaluation continues past a recorded stop, so a later mismatch is a
        // consequence; reporting it instead would name the wrong site.
        let env = EvalEnv::empty();
        malformed_elimination("Fst", &Term::Zero, &env);
        malformed_elimination("Unfold", &Term::Unit, &env);
        let Some(EvalStopped::MalformedElimination { eliminator, .. }) = env.recorded_stop() else {
            panic!("expected a recorded elimination mismatch");
        };
        assert_eq!(eliminator, "Fst");
    }

    #[test]
    fn every_shape_is_named_distinctly_enough_to_act_on() {
        assert_eq!(value_shape(&Term::Unit), "Unit");
        assert_eq!(value_shape(&Term::True), "a Bool");
        assert_eq!(
            value_shape(&Term::Pair(Box::new(Term::Unit), Box::new(Term::Unit))),
            "a pair"
        );
        // A `Spanned` wrapper must not hide the shape underneath it.
        assert_eq!(
            value_shape(&Term::Spanned(
                Box::new(Term::Zero),
                crate::terms::TermSpan::new(0, 0)
            )),
            "a Nat"
        );
    }
}
