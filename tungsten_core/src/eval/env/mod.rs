//! Environment-based evaluation with call-by-need semantics
//!
//! This module provides an evaluator that uses an environment to look up
//! global definitions. Lookups are memoized to provide call-by-need
//! (lazy evaluation with sharing), avoiding exponential term blowup.

mod comparator_stop;
mod deadline;
pub(crate) mod handlers;
mod handlers_string;
mod helpers;
mod lookup;
mod stopped;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

pub use comparator_stop::{ComparatorFailure, ComparatorFailureKind};
pub use deadline::eval_with_env_until;
// The evaluator's executable-extern registry, surfaced for `tungsten info eval
// externs` / `doctor check extern-coverage` (ADR 28.7.26a retrospective).
pub use handlers::externs::registry as extern_registry;
pub use stopped::{EvalStopped, GlobalLookup, IntTrapKind};

use crate::terms::Term;
use crate::types::Type;

use super::StepResult;

/// Lazy comparator synthesis callback (ADR 29.6.26f §T11.2a / P6′).
///
/// Given a concrete type `T`, returns `(top_symbol, defs)` where `top_symbol` is
/// the comparator symbol for `T` and `defs` are all `(symbol, body)` comparators
/// in its transitive closure (so sub-comparators resolve). Supplied by
/// `bootstrap` (which owns the synthesis) so the evaluator can resolve
/// `__cmp<T>` without depending on `bootstrap`.
///
/// The error side is a [`ComparatorFailure`], not a bare `None` (ADR 1.8.26b
/// D3). The distinction is load-bearing: the pre-1.8.26b callback decided
/// comparability by `closure.is_empty()`, and the D2 defect produces a
/// **non-empty** closure whose body calls a symbol the closure never defines —
/// so an emptiness predicate returned `Some`, the evaluator installed the defs,
/// and the unbound recursive edge went Stuck exactly as if nothing had
/// happened. The callback must therefore validate what it is about to hand
/// back, and say *which* way it failed.
pub type ComparatorSynth =
    Rc<dyn Fn(&Type) -> Result<(String, Vec<(String, Term)>), ComparatorFailure>>;

use handlers::dispatch::{step_arith_bool_env, step_core_env, step_return_env, step_string_env};
use handlers::int_ops::step_int_env;
use handlers::{step_adt_match_env, step_extern_call_env};
use helpers::{step_eval_then_env, step_eval_to_value_env};

// ============================================================================
// EvalEnv
// ============================================================================

/// Evaluation environment mapping global names to definitions
///
/// The environment provides call-by-need semantics: when a global is first
/// looked up, its definition is evaluated to a value and cached. Subsequent
/// lookups return the cached value directly.
#[derive(Clone)]
pub struct EvalEnv {
    /// Map from global names to their unevaluated definitions
    globals: HashMap<String, Term>,
    /// Comparators synthesized on demand (`__cmp<T>` resolution, ADR 29.6.26f).
    dynamic: RefCell<HashMap<String, Term>>,
    /// Cache of already-evaluated values (for call-by-need)
    cache: RefCell<HashMap<String, Term>>,
    /// Lazy comparator synthesis callback (`None` when not configured).
    comparator_synth: Option<ComparatorSynth>,
    /// Globals currently being forced, innermost last (ADR 22.7.26a / D1).
    /// Ordered (not a set) so a detected cycle can be reported as a path.
    forcing: RefCell<Vec<String>>,
    /// The first detected black-hole cycle, if any (ADR 22.7.26a / D2).
    black_hole: RefCell<Option<Vec<String>>>,
    /// The first recorded stop, if any: a comparison that never ran (ADR
    /// 1.8.26b / D3), a malformed elimination, or an `Int` trap (ADR
    /// 14.9.26c). Rides the env for the same reason `black_hole` does:
    /// interior stepping must keep returning `Stuck` so the loops terminate,
    /// but the reporting boundary must not mistake the residual for a value.
    recorded_stop: RefCell<Option<EvalStopped>>,
    /// How many test assertions actually EXECUTED during this evaluation
    /// (ADR 6.8.26b). Counted rather than inferred: a test that finishes with
    /// zero here asserted nothing, whatever the reason, and reporting it `ok`
    /// is the defect. Rides the env like `comparator_stop` because the
    /// reporting boundary — not the stepping loop — is what needs the answer.
    assertions_executed: Cell<u64>,
    /// How many EFFECTFUL externs executed during this evaluation (ADR
    /// 14.9.26a). `lookup` reads it around each forcing: a global whose body
    /// performed an effect is a *call* natively — every reference runs it
    /// again — so memoizing its value would share one effect across every
    /// use. `string_builder_new` was the first case where that sharing is
    /// observably wrong (one builder aliased by every `new`), but the class
    /// is every nullary function with a side effect.
    effects_performed: Cell<u64>,
}

impl std::fmt::Debug for EvalEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalEnv")
            .field("globals", &self.globals.keys().collect::<Vec<_>>())
            .field("comparator_synth", &self.comparator_synth.is_some())
            .finish_non_exhaustive()
    }
}

impl EvalEnv {
    /// Create a new environment from a map of global definitions
    #[must_use]
    pub fn new(globals: HashMap<String, Term>) -> Self {
        EvalEnv {
            globals,
            dynamic: RefCell::new(HashMap::new()),
            cache: RefCell::new(HashMap::new()),
            comparator_synth: None,
            forcing: RefCell::new(Vec::new()),
            black_hole: RefCell::new(None),
            recorded_stop: RefCell::new(None),
            assertions_executed: Cell::new(0),
            effects_performed: Cell::new(0),
        }
    }

    /// Create an empty environment
    #[must_use]
    pub fn empty() -> Self {
        EvalEnv::new(HashMap::new())
    }

    /// Install the lazy comparator synthesis callback (ADR 29.6.26f §T11.2a).
    #[must_use]
    pub fn with_comparator_synth(mut self, synth: ComparatorSynth) -> Self {
        self.comparator_synth = Some(synth);
        self
    }

    /// The configured comparator synthesis callback, if any.
    #[must_use]
    pub fn comparator_synth(&self) -> Option<&ComparatorSynth> {
        self.comparator_synth.as_ref()
    }

    /// Register on-demand synthesized comparator definitions (idempotent).
    pub fn register_comparators(&self, defs: Vec<(String, Term)>) {
        let mut dynamic = self.dynamic.borrow_mut();
        for (name, body) in defs {
            dynamic.entry(name).or_insert(body);
        }
    }

    /// The names of comparators synthesized into this env so far.
    ///
    /// A caller diagnosing a stuck residual needs these: they are *defined*,
    /// but they are not in the static def list, so a diagnosis that resolves
    /// globals against that list alone reports them "never resolved" and sends
    /// the reader after the wrong cause (measured during ADR 1.8.26b's D2
    /// diagnosis, where it cost the session its first root-cause thesis).
    #[must_use]
    pub fn registered_comparators(&self) -> Vec<String> {
        self.dynamic.borrow().keys().cloned().collect()
    }

    // `record_stop` / `recorded_stop` live in `stopped.rs`.

    /// Record that one test assertion actually executed (ADR 6.8.26b).
    ///
    /// Called from the assertion dispatch in `handlers/externs/call.rs`, which
    /// is the single funnel every `.tg` assertion reduces through — so this
    /// counts the whole `assert_eq`/`assert_ne`/`assert`/`fail` surface
    /// without knowing how many wrappers sit above it.
    pub fn record_assertion_executed(&self) {
        self.assertions_executed
            .set(self.assertions_executed.get().saturating_add(1));
    }

    /// How many test assertions executed during this evaluation.
    ///
    /// **Zero means the test asserted nothing** — the outcome that used to be
    /// reported `ok`, because an assertion that never runs never sets the
    /// failure flag either.
    /// (The runner builds a fresh env per test, so this needs no reset.)
    #[must_use]
    pub fn assertions_executed(&self) -> u64 {
        self.assertions_executed.get()
    }

    /// Record that an effectful extern executed (ADR 14.9.26a).
    ///
    /// Called by the extern dispatcher for every claimed call whose registry
    /// kind is not `Pure`; `lookup` compares the count before and after
    /// forcing a global and refuses to memoize one that moved it.
    pub fn record_effect_performed(&self) {
        self.effects_performed
            .set(self.effects_performed.get().saturating_add(1));
    }

    /// How many effectful externs have executed on this env so far.
    #[must_use]
    pub fn effects_performed(&self) -> u64 {
        self.effects_performed.get()
    }

    // `lookup` (call-by-need forcing + black-hole detection) lives in
    // `lookup.rs` (ADR 22.7.26a).
}

// ============================================================================
// Environment-based evaluation
// ============================================================================

/// Evaluate a term to a value using the given environment.
///
/// `Err(EvalStopped::BlackHole)` — the only stop this unbounded entry can
/// produce — means a global re-entered its own forcing (ADR 22.7.26a); the
/// term never had a value and no caller may treat the result as one.
pub fn eval_with_env(term: &Term, env: &EvalEnv) -> Result<Term, EvalStopped> {
    finished_or_black_hole(eval_term_loop(term, env), env)
}

/// Resolve a finished (value-or-stuck) term against the env's recorded stops:
/// a black-hole cycle (ADR 22.7.26a / D2) or a comparison that never ran (ADR
/// 1.8.26b / D3) wins over the surface term, which is a poisoned partial
/// result whenever either was detected.
///
/// Black holes are consulted first only to preserve the pre-1.8.26b diagnosis
/// on a term that manages to hit both; the two are independent.
pub(super) fn finished_or_black_hole(finished: Term, env: &EvalEnv) -> Result<Term, EvalStopped> {
    if let Some(cycle) = env.black_hole_cycle() {
        return Err(EvalStopped::BlackHole { cycle });
    }
    match env.recorded_stop() {
        Some(stop) => Err(stop),
        None => Ok(finished),
    }
}

/// The raw stepping loop shared by the entry points and `EvalEnv::lookup`.
///
/// Interior-only: a black hole surfaces here as an ordinary stuck term (the
/// loop must terminate), and the recorded cycle rides the env — callers other
/// than `lookup` must check it, which the `eval_with_env*` entries do.
fn eval_term_loop(term: &Term, env: &EvalEnv) -> Term {
    let mut current = term.strip_spans();
    loop {
        match step_with_env(&current, env) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => return current,
        }
    }
}

/// Evaluate with environment and step limit.
///
/// `Err(EvalStopped::StepLimit)` replaces the old `None`; a black hole
/// detected below the limit reports as `BlackHole`, never as `StepLimit` —
/// the exhaustion is a consequence, not the diagnosis.
pub fn eval_with_env_and_limit(
    term: &Term,
    env: &EvalEnv,
    limit: usize,
) -> Result<Term, EvalStopped> {
    let mut current = term.strip_spans();
    for _ in 0..limit {
        match step_with_env(&current, env) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => {
                return finished_or_black_hole(current, env);
            }
        }
    }
    // Limit exhausted; a recorded stop (poisoned env) is the truer diagnosis.
    if let Some(cycle) = env.black_hole_cycle() {
        return Err(EvalStopped::BlackHole { cycle });
    }
    if let Some(stop) = env.recorded_stop() {
        return Err(stop);
    }
    Err(EvalStopped::StepLimit { limit })
}

// ============================================================================
// step_with_env - Environment-based stepper
// ============================================================================

/// Perform one step of call-by-value evaluation with environment
///
/// This is the environment-aware version of `step()`. Global references
/// are resolved through the environment with call-by-need memoization.
pub fn step_with_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        // Values
        Term::Lambda(_, _, _)
        | Term::TyAbs(_, _)
        | Term::True
        | Term::False
        | Term::Unit
        | Term::Zero
        | Term::NatLit(_)
        | Term::IntLit(_)
        | Term::StringLit(_) => StepResult::Value,

        // Test-assertion FFIs execute; other extern calls stay stuck.
        Term::ExternCall(name, args) => step_extern_call_env(name, args, env),

        // Stuck terms
        Term::Var(_) | Term::Sorry | Term::RefNew(_) | Term::RefGet(_) | Term::RefSet(_, _) => {
            StepResult::Stuck
        }

        // Global lookup. A black hole is stuck *interiorly* (the loop must
        // terminate) but never at the reporting boundary: the cycle rides the
        // env and the `eval_with_env*` entries return it as a distinct
        // outcome (ADR 22.7.26a / D2).
        Term::Global(name) => match env.lookup(name) {
            GlobalLookup::Value(value) => StepResult::Stepped(value),
            GlobalLookup::Unbound | GlobalLookup::BlackHole => StepResult::Stuck,
        },

        // Evaluate-to-value wrappers
        Term::Succ(t) => step_eval_to_value_env(t, Term::succ, env),
        Term::Inl(ty, t) => step_eval_to_value_env(t, |t_new| Term::inl(ty.clone(), t_new), env),
        Term::Inr(ty, t) => step_eval_to_value_env(t, |t_new| Term::inr(ty.clone(), t_new), env),
        Term::Refl(ty, t) => step_eval_to_value_env(t, |t_new| Term::refl(ty.clone(), t_new), env),
        Term::Fold(ty, t) => step_eval_to_value_env(t, |t_new| Term::fold(ty.clone(), t_new), env),
        Term::AdtConstruct(adt_ty, idx, payload) => step_eval_to_value_env(
            payload,
            |p_new| Term::adt_construct(adt_ty.clone(), *idx, p_new),
            env,
        ),

        // Evaluate-then-stuck wrapper
        Term::Absurd(ty, t) => step_eval_then_env(t, |t_new| Term::absurd(ty.clone(), t_new), env),

        // Core lambda calculus, products, sums, proof/recursion
        Term::App(..)
        | Term::Let(..)
        | Term::If(..)
        | Term::TyApp(..)
        | Term::Annot(..)
        | Term::Fix(..)
        | Term::Pair(..)
        | Term::Fst(_)
        | Term::Snd(_)
        | Term::Case(..)
        | Term::NatRec(..)
        | Term::NatInd(..)
        | Term::Subst(..)
        | Term::Unfold(..) => step_core_env(term, env),

        // String operations
        Term::StrConcat(..)
        | Term::StrLen(_)
        | Term::StrEq(..)
        | Term::StrCharAt(..)
        | Term::StrSubstring(..) => step_string_env(term, env),

        // Arithmetic, boolean, and nat comparison operations
        Term::NatAdd(..)
        | Term::NatSub(..)
        | Term::NatMul(..)
        | Term::NatDiv(..)
        | Term::NatMod(..)
        | Term::NatEq(..)
        | Term::NatLt(..)
        | Term::NatLe(..)
        | Term::NatGt(..)
        | Term::NatGe(..)
        | Term::BoolAnd(..)
        | Term::BoolOr(..)
        | Term::BoolNot(_) => step_arith_bool_env(term, env),

        // Signed integers: checked, trapping (ADR 14.9.26c)
        Term::IntBin(..) | Term::IntNeg(_) | Term::NatToInt(_) | Term::IntToNat(_) => {
            step_int_env(term, env)
        }

        // ADT match
        Term::AdtMatch(scrut, arms) => step_adt_match_env(scrut, arms, env),

        // Span wrapper: strip and step inner term
        Term::Spanned(inner, _) => step_with_env(inner, env),

        // Return: evaluate inner, then strip the Return wrapper
        Term::Return(t) => step_return_env(t, env),
    }
}

// Tests
#[cfg(test)]
mod tests;
