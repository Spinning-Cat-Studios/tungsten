//! Global lookup: call-by-need memoization + black-hole detection
//! (ADR 22.7.26a).
//!
//! `EvalEnv::lookup` memoizes a global only *after* the evaluation that
//! computes it, so a global whose body reaches itself in evaluation position
//! re-enters `lookup` with the cache still empty — pre-detection that was
//! unbounded Rust recursion and a stack overflow in milliseconds, far inside
//! any wall-clock watchdog (ADR 21.7.26f §4.1). Detection: an insertion-
//! ordered in-progress stack on the env, an RAII frame guard, and a recorded
//! cycle that rides the env to the entry points.
//!
//! Soundness (ADR 22.7.26a / D4): under the current evaluator every
//! constructor is strict (`Succ`/`Inl`/`Inr`/`Refl`/`Fold`/`AdtConstruct`
//! route through `step_eval_to_value_env`, `Pair` through `step_pair_env`),
//! so there is no lazy constructor under which a self-reference could be
//! guarded and still make progress: a re-entered global is always
//! non-productive. If Tungsten ever gains productive corecursion (a *lazy*
//! `f = cons 1 f`), this detection must learn to admit progress through a
//! constructor — that is a precondition of the design, not an oversight.

use super::stopped::GlobalLookup;
use super::{eval_term_loop, EvalEnv};

/// RAII frame marking a global as currently being forced.
///
/// The entry must leave `forcing` on **every** exit path — an early return or
/// a panic-unwind through `lookup` would otherwise leak the name and turn the
/// *next* legitimate lookup (the diamond case: `a` and `b` both forcing `c`)
/// into a false-positive black hole (ADR 22.7.26a / D1).
struct ForcingFrame<'env> {
    env: &'env EvalEnv,
}

impl<'env> ForcingFrame<'env> {
    /// Push `name` onto the env's in-progress stack.
    fn enter(env: &'env EvalEnv, name: &str) -> Self {
        env.forcing.borrow_mut().push(name.to_string());
        ForcingFrame { env }
    }
}

impl Drop for ForcingFrame<'_> {
    fn drop(&mut self) {
        // Forcing is strictly nested (lookup → eval → lookup …), so the top
        // of the stack is always this frame's name.
        self.env.forcing.borrow_mut().pop();
    }
}

impl EvalEnv {
    /// Look up a global, evaluating and caching if necessary. Consults the
    /// static `globals` and the on-demand `dynamic` (synthesized) definitions.
    ///
    /// Re-entering a name already being forced is a black hole (ADR 22.7.26a):
    /// the cycle is recorded on the env; interior stepping treats the result
    /// as stuck and the entry points report it as `EvalStopped::BlackHole`.
    pub fn lookup(&self, name: &str) -> GlobalLookup {
        if let Some(cached) = self.cache.borrow().get(name) {
            return GlobalLookup::Value(cached.clone());
        }
        if self.forcing.borrow().iter().any(|forced| forced == name) {
            self.record_black_hole_cycle(name);
            return GlobalLookup::BlackHole;
        }

        // Clone the def out before evaluating (eval may itself register more
        // dynamic comparators, so we must not hold a borrow across eval).
        let def = self
            .dynamic
            .borrow()
            .get(name)
            .or_else(|| self.globals.get(name))
            .cloned();
        let Some(def) = def else {
            return GlobalLookup::Unbound;
        };
        let effects_before = self.effects_performed();
        let value = {
            let _frame = ForcingFrame::enter(self, name);
            eval_term_loop(&def, self)
        };
        if self.black_hole.borrow().is_some() {
            // The forcing hit a cycle somewhere below: `value` is a poisoned
            // partial term, not this global's value. Never memoize it.
            return GlobalLookup::BlackHole;
        }
        if self.effects_performed() != effects_before {
            // The forcing performed an effect, so this global is a CALL, not
            // a value: natively every reference runs the body again, and a
            // memo here would share one effect across all of them — one
            // `string_builder_new` handle aliased by every `new` in the
            // program (ADR 14.9.26a). Hand the value back unmemoized; the
            // next reference forces it afresh. The counter is monotone, so a
            // nullary wrapper over an effectful extern is caught transitively.
            return GlobalLookup::Value(value);
        }
        self.cache
            .borrow_mut()
            .insert(name.to_string(), value.clone());
        GlobalLookup::Value(value)
    }

    /// Record the ordered cycle for a re-entered global. First cycle wins:
    /// once the env is black-holed, later detections are consequences of the
    /// first and would only obscure it.
    fn record_black_hole_cycle(&self, reentered: &str) {
        let mut recorded = self.black_hole.borrow_mut();
        if recorded.is_some() {
            return;
        }
        let forcing = self.forcing.borrow();
        let cycle_start = forcing
            .iter()
            .position(|forced| forced == reentered)
            .unwrap_or(0);
        let mut cycle: Vec<String> = forcing[cycle_start..].to_vec();
        cycle.push(reentered.to_string());
        *recorded = Some(cycle);
    }

    /// The recorded black-hole cycle, if any forcing re-entered itself.
    ///
    /// The flag is never cleared: an env that produced a black hole is
    /// poisoned (some lookup returned a non-value), and every subsequent
    /// entry-point result on it is suspect.
    #[must_use]
    pub fn black_hole_cycle(&self) -> Option<Vec<String>> {
        self.black_hole.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::eval::env::{
        eval_with_env, eval_with_env_and_limit, eval_with_env_until, EvalEnv, EvalStopped,
        GlobalLookup,
    };
    use crate::terms::Term;
    use crate::types::Type;

    fn env_of(globals: &[(&str, Term)]) -> EvalEnv {
        EvalEnv::new(
            globals
                .iter()
                .map(|(name, term)| ((*name).to_string(), term.clone()))
                .collect::<HashMap<_, _>>(),
        )
    }

    /// AC 1 (in-process half): post-fix, `lookup` on a self-referential
    /// global returns the black-hole state without recursing — this test
    /// completing at all is the "no stack overflow" assertion.
    #[test]
    fn self_referential_global_is_a_black_hole_not_a_stack_overflow() {
        let env = env_of(&[("loop_forever", Term::Global("loop_forever".into()))]);
        assert_eq!(env.lookup("loop_forever"), GlobalLookup::BlackHole);
        assert_eq!(
            env.black_hole_cycle(),
            Some(vec!["loop_forever".to_string(), "loop_forever".to_string()]),
            "the cycle must be recorded, ordered and closed"
        );
    }

    /// D2: every entry point returns the black hole as a distinct outcome,
    /// never a silently-Ok stuck term.
    #[test]
    fn all_three_entry_points_report_the_black_hole() {
        let expected = Err(EvalStopped::BlackHole {
            cycle: vec!["f".to_string(), "f".to_string()],
        });
        let term = Term::Global("f".into());
        let globals = [("f", Term::Global("f".into()))];

        assert_eq!(eval_with_env(&term, &env_of(&globals)), expected);
        assert_eq!(
            eval_with_env_and_limit(&term, &env_of(&globals), 100),
            expected
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        assert_eq!(
            eval_with_env_until(&term, &env_of(&globals), deadline),
            expected
        );
    }

    /// A mutual cycle reports the ordered path, not a bare name (D2).
    #[test]
    fn mutual_cycle_reports_the_ordered_path() {
        let env = env_of(&[
            ("a", Term::Global("b".into())),
            ("b", Term::Global("a".into())),
        ]);
        assert_eq!(
            eval_with_env(&Term::Global("a".into()), &env),
            Err(EvalStopped::BlackHole {
                cycle: vec!["a".to_string(), "b".to_string(), "a".to_string()],
            })
        );
    }

    /// AC "no false positives" (a): the diamond — `a` and `b` both force `c`.
    /// `c` must leave the in-progress set when its forcing completes, so the
    /// second consumer finds it cached, not in-progress.
    #[test]
    fn diamond_forcing_is_not_a_false_positive() {
        let env = env_of(&[
            ("c", Term::nat_add(Term::NatLit(1), Term::NatLit(1))),
            ("a", Term::Global("c".into())),
            ("b", Term::Global("c".into())),
            (
                "main",
                Term::nat_add(Term::Global("a".into()), Term::Global("b".into())),
            ),
        ]);
        let value = eval_with_env(&Term::Global("main".into()), &env)
            .expect("the diamond must not be flagged as a black hole");
        assert_eq!(crate::eval::term_to_nat(&value), Some(4));
        assert_eq!(env.black_hole_cycle(), None);
    }

    /// AC "no false positives" (b): a genuinely self-referential global whose
    /// body is a *lambda* — the shape `register_comparators` mints for
    /// recursive synthesized comparators (`compare_List` reaching
    /// `compare_List`, ADR 29.6.26f). A lambda is a value, so forcing it
    /// never re-enters; it must memoize exactly as today (D4).
    #[test]
    fn recursive_synthesized_comparator_is_not_a_false_positive() {
        let env = EvalEnv::empty();
        let self_calling_comparator = Term::lambda(
            "x",
            Type::Nat,
            Term::app(Term::Global("compare_List".into()), Term::var("x")),
        );
        env.register_comparators(vec![(
            "compare_List".to_string(),
            self_calling_comparator.clone(),
        )]);

        assert_eq!(
            env.lookup("compare_List"),
            GlobalLookup::Value(self_calling_comparator),
            "a self-referential lambda memoizes as a closure, not a black hole"
        );
        assert_eq!(env.black_hole_cycle(), None);
    }

    /// D4's first attempted falsifier: `def f = Pair(f, 1)` *looks*
    /// corecursive, but `Pair` is strict, so it is a genuine black hole.
    #[test]
    fn strict_pair_self_reference_is_a_genuine_black_hole() {
        let env = env_of(&[("f", Term::pair(Term::Global("f".into()), Term::NatLit(1)))]);
        assert!(matches!(
            eval_with_env(&Term::Global("f".into()), &env),
            Err(EvalStopped::BlackHole { .. })
        ));
    }

    /// AC "Stuck is unchanged": an unbound global — the arm sharing
    /// `lookup`'s code path, most at risk of accidental reclassification —
    /// still evaluates to an ordinary stuck term, reported Ok.
    #[test]
    fn unbound_global_stays_an_ordinary_stuck_term() {
        let env = EvalEnv::empty();
        assert_eq!(env.lookup("missing"), GlobalLookup::Unbound);
        let term = Term::Global("missing".into());
        assert_eq!(eval_with_env(&term, &env), Ok(term.clone()));
        assert_eq!(env.black_hole_cycle(), None);
    }

    /// AC "Stuck is unchanged": the named stuck cases still evaluate to
    /// themselves as ordinary Ok results.
    #[test]
    fn named_stuck_cases_still_evaluate_as_today() {
        let env = EvalEnv::empty();
        let stuck_cases = [
            Term::var("x"),
            Term::Sorry,
            Term::RefNew(Box::new(Term::Zero)),
            Term::RefGet(Box::new(Term::var("r"))),
            Term::RefSet(Box::new(Term::var("r")), Box::new(Term::Zero)),
            Term::ExternCall("__c_not_a_test_ffi".to_string(), vec![]),
        ];
        for stuck in stuck_cases {
            assert_eq!(eval_with_env(&stuck, &env), Ok(stuck.clone()));
        }
    }

    /// Call-by-need still shares: a global forced twice is evaluated once
    /// (the second lookup is served from cache).
    #[test]
    fn call_by_need_memoization_still_shares() {
        let env = env_of(&[("g", Term::nat_add(Term::NatLit(2), Term::NatLit(3)))]);
        let first = env.lookup("g");
        let GlobalLookup::Value(forced) = &first else {
            panic!("forcing g must produce a value, got {first:?}");
        };
        assert_eq!(crate::eval::term_to_nat(forced), Some(5));
        assert!(env.cache.borrow().contains_key("g"), "g must be memoized");
        assert_eq!(env.lookup("g"), first, "second lookup serves the memo");
    }

    /// A global whose forcing performed an effect is NOT memoized: each
    /// reference forces it afresh, as each native call runs the body again
    /// (ADR 14.9.26a). `tg_string_builder_new` is the instance — under a memo
    /// every `new` in one program aliased one builder, and the second
    /// `to_string` aborted the evaluator on a handle the first had consumed.
    #[test]
    fn a_global_that_performs_an_effect_is_forced_on_every_reference() {
        let env = env_of(&[(
            "fresh",
            Term::ExternCall("tg_string_builder_new".into(), vec![]),
        )]);
        let first = env.lookup("fresh");
        let second = env.lookup("fresh");
        assert!(
            !env.cache.borrow().contains_key("fresh"),
            "an effectful global must not be memoized"
        );
        assert!(matches!(first, GlobalLookup::Value(_)));
        assert_ne!(first, second, "each reference must yield a fresh handle");
        assert_eq!(env.effects_performed(), 2, "one effect per forcing");
    }

    /// A black-holed env is poisoned: even a step-limit exhaustion on a later
    /// evaluation reports the black hole, never `StepLimit` — the exhaustion
    /// is a consequence, not the diagnosis.
    #[test]
    fn poisoned_env_reports_black_hole_over_step_limit() {
        let env = env_of(&[("f", Term::Global("f".into()))]);
        assert!(eval_with_env(&Term::Global("f".into()), &env).is_err());

        // `fix g. g` steps forever, so a limit of 5 exhausts — but the env
        // already has a recorded cycle, which is the truer diagnosis.
        let spinning = Term::fix("g", Type::Nat, Term::var("g"));
        assert!(matches!(
            eval_with_env_and_limit(&spinning, &env, 5),
            Err(EvalStopped::BlackHole { .. })
        ));
    }

    /// A clean env still reports StepLimit on exhaustion (unchanged contract,
    /// new spelling: `Err(StepLimit)` where `None` used to be).
    #[test]
    fn step_limit_exhaustion_on_a_clean_env_reports_step_limit() {
        let env = EvalEnv::empty();
        let spinning = Term::fix("g", Type::Nat, Term::var("g"));
        assert_eq!(
            eval_with_env_and_limit(&spinning, &env, 5),
            Err(EvalStopped::StepLimit { limit: 5 })
        );
    }

    /// The black hole names its cycle in the human-readable rendering used by
    /// the reporting layers (`tungsten run`'s diagnostic, the FFI error).
    #[test]
    fn display_rendering_names_the_cycle() {
        let stopped = EvalStopped::BlackHole {
            cycle: vec!["a".to_string(), "b".to_string(), "a".to_string()],
        };
        assert_eq!(stopped.to_string(), "black hole: a → b → a");
        assert_eq!(
            EvalStopped::StepLimit { limit: 7 }.to_string(),
            "evaluation exceeded 7 steps"
        );
        assert_eq!(
            EvalStopped::TimedOut { steps: 100 }.to_string(),
            "evaluation timed out (~100 steps)"
        );
    }
}
