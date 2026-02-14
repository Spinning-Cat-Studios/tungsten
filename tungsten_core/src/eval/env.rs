//! Environment-based evaluation with call-by-need semantics
//!
//! This module provides an evaluator that uses an environment to look up
//! global definitions. Lookups are memoized to provide call-by-need
//! (lazy evaluation with sharing), avoiding exponential term blowup.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::terms::Term;

use super::helpers::{nat_to_term, term_to_nat};
use super::StepResult;

// ============================================================================
// EvalEnv
// ============================================================================

/// Evaluation environment mapping global names to definitions
///
/// The environment provides call-by-need semantics: when a global is first
/// looked up, its definition is evaluated to a value and cached. Subsequent
/// lookups return the cached value directly.
///
/// ## Example
///
/// ```ignore
/// let mut env = EvalEnv::new();
/// env.insert("id".to_string(), Term::lambda("x", Type::Nat, Term::var("x")));
/// env.insert("main".to_string(), Term::app(Term::Global("id".into()), Term::Zero));
///
/// let result = eval_with_env(&Term::Global("main".into()), &env);
/// // result == Term::Zero
/// ```
#[derive(Debug, Clone)]
pub struct EvalEnv {
    /// Map from global names to their unevaluated definitions
    globals: HashMap<String, Term>,
    /// Cache of already-evaluated values (for call-by-need)
    cache: RefCell<HashMap<String, Term>>,
}

impl EvalEnv {
    /// Create a new environment from a map of global definitions
    #[must_use]
    pub fn new(globals: HashMap<String, Term>) -> Self {
        EvalEnv {
            globals,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Create an empty environment
    #[must_use]
    pub fn empty() -> Self {
        EvalEnv::new(HashMap::new())
    }

    /// Look up a global, evaluating and caching if necessary
    ///
    /// This provides call-by-need semantics: the first lookup evaluates the
    /// definition to a value and caches it. Subsequent lookups return the
    /// cached value.
    pub fn lookup(&self, name: &str) -> Option<Term> {
        // Check cache first
        if let Some(cached) = self.cache.borrow().get(name) {
            return Some(cached.clone());
        }

        // Look up the definition
        if let Some(def) = self.globals.get(name) {
            // Evaluate it to a value (recursive call with same env)
            let value = eval_with_env(def, self);

            // Cache the result
            self.cache
                .borrow_mut()
                .insert(name.to_string(), value.clone());

            Some(value)
        } else {
            None
        }
    }
}

// ============================================================================
// Environment-based evaluation
// ============================================================================

/// Evaluate a term to a value using the given environment
///
/// This is the recommended entry point for evaluating programs with
/// global definitions. The environment maps global names to their
/// definitions, and lookups are memoized for efficiency.
pub fn eval_with_env(term: &Term, env: &EvalEnv) -> Term {
    let mut current = term.clone();
    loop {
        match step_with_env(&current, env) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => return current,
        }
    }
}

/// Evaluate with environment and step limit
///
/// Returns `None` if the step limit is exceeded.
pub fn eval_with_env_and_limit(term: &Term, env: &EvalEnv, limit: usize) -> Option<Term> {
    let mut current = term.clone();
    for _ in 0..limit {
        match step_with_env(&current, env) {
            StepResult::Stepped(next) => current = next,
            StepResult::Value | StepResult::Stuck => return Some(current),
        }
    }
    None
}

// ============================================================================
// Helper functions for binary/unary operations with environment
// ============================================================================

/// Helper for binary Nat operations with environment
fn step_binary_nat_env<F, C>(a: &Term, b: &Term, op: F, constructor: C, env: &EvalEnv) -> StepResult
where
    F: FnOnce(usize, usize) -> usize,
    C: FnOnce(Term, Term) -> Term,
{
    // Step left operand if not a value
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new, b.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Step right operand if not a value
    if !b.is_value() {
        match step_with_env(b, env) {
            StepResult::Stepped(b_new) => {
                return StepResult::Stepped(constructor(a.clone(), b_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Both are values, compute
    match (term_to_nat(a), term_to_nat(b)) {
        (Some(x), Some(y)) => StepResult::Stepped(nat_to_term(op(x, y))),
        _ => StepResult::Stuck,
    }
}

/// Helper for binary Nat->Bool operations with environment
fn step_binary_nat_to_bool_env<F, C>(
    a: &Term,
    b: &Term,
    op: F,
    constructor: C,
    env: &EvalEnv,
) -> StepResult
where
    F: FnOnce(usize, usize) -> bool,
    C: FnOnce(Term, Term) -> Term,
{
    // Step left operand if not a value
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new, b.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Step right operand if not a value
    if !b.is_value() {
        match step_with_env(b, env) {
            StepResult::Stepped(b_new) => {
                return StepResult::Stepped(constructor(a.clone(), b_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Both are values, compute
    match (term_to_nat(a), term_to_nat(b)) {
        (Some(x), Some(y)) => {
            if op(x, y) {
                StepResult::Stepped(Term::True)
            } else {
                StepResult::Stepped(Term::False)
            }
        }
        _ => StepResult::Stuck,
    }
}

/// Helper for binary Bool operations with environment
fn step_binary_bool_env<F, C>(
    a: &Term,
    b: &Term,
    op: F,
    constructor: C,
    env: &EvalEnv,
) -> StepResult
where
    F: FnOnce(bool, bool) -> bool,
    C: FnOnce(Term, Term) -> Term,
{
    // Step left operand if not a value
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new, b.clone()));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Step right operand if not a value
    if !b.is_value() {
        match step_with_env(b, env) {
            StepResult::Stepped(b_new) => {
                return StepResult::Stepped(constructor(a.clone(), b_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Both are values, compute
    match (a, b) {
        (Term::True, Term::True) => StepResult::Stepped(if op(true, true) {
            Term::True
        } else {
            Term::False
        }),
        (Term::True, Term::False) => StepResult::Stepped(if op(true, false) {
            Term::True
        } else {
            Term::False
        }),
        (Term::False, Term::True) => StepResult::Stepped(if op(false, true) {
            Term::True
        } else {
            Term::False
        }),
        (Term::False, Term::False) => StepResult::Stepped(if op(false, false) {
            Term::True
        } else {
            Term::False
        }),
        _ => StepResult::Stuck,
    }
}

/// Helper for unary Bool operations with environment
fn step_unary_bool_env<F, C>(a: &Term, op: F, constructor: C, env: &EvalEnv) -> StepResult
where
    F: FnOnce(bool) -> bool,
    C: FnOnce(Term) -> Term,
{
    // Step operand if not a value
    if !a.is_value() {
        match step_with_env(a, env) {
            StepResult::Stepped(a_new) => {
                return StepResult::Stepped(constructor(a_new));
            }
            StepResult::Stuck => return StepResult::Stuck,
            StepResult::Value => {}
        }
    }

    // Operand is a value, compute
    match a {
        Term::True => StepResult::Stepped(if op(true) { Term::True } else { Term::False }),
        Term::False => StepResult::Stepped(if op(false) { Term::True } else { Term::False }),
        _ => StepResult::Stuck,
    }
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
        // Global references are looked up in the environment
        Term::Global(name) => {
            match env.lookup(name) {
                Some(value) => StepResult::Stepped(value),
                None => StepResult::Stuck, // Undefined global
            }
        }

        // Variables are stuck (open term)
        Term::Var(_) => StepResult::Stuck,

        // Lambda is a value
        Term::Lambda(_, _, _) => StepResult::Value,

        // Application: evaluate to get function, then argument, then β-reduce
        Term::App(t1, t2) => {
            // If t1 is not a value, step it
            if !t1.is_value() {
                match step_with_env(t1, env) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::app(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {} // continue
                }
            }

            // If t2 is not a value, step it
            if !t2.is_value() {
                match step_with_env(t2, env) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::app(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {} // continue
                }
            }

            // Both are values: β-reduce if t1 is a lambda
            match t1.as_ref() {
                Term::Lambda(x, _ty, body) => {
                    let result = body.substitute(x, t2);
                    StepResult::Stepped(result)
                }
                _ => StepResult::Stuck,
            }
        }

        // Let: evaluate definition, then substitute
        Term::Let(x, _ty, def, body) => {
            if !def.is_value() {
                match step_with_env(def, env) {
                    StepResult::Stepped(def_new) => {
                        return StepResult::Stepped(Term::let_in(
                            x.clone(),
                            _ty.clone(),
                            def_new,
                            body.as_ref().clone(),
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            // def is a value, substitute into body
            let result = body.substitute(x, def);
            StepResult::Stepped(result)
        }

        // Boolean values
        Term::True => StepResult::Value,
        Term::False => StepResult::Value,

        // If-then-else
        Term::If(cond, then_, else_) => {
            if !cond.is_value() {
                match step_with_env(cond, env) {
                    StepResult::Stepped(cond_new) => {
                        return StepResult::Stepped(Term::if_then_else(
                            cond_new,
                            then_.as_ref().clone(),
                            else_.as_ref().clone(),
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match cond.as_ref() {
                Term::True => StepResult::Stepped(then_.as_ref().clone()),
                Term::False => StepResult::Stepped(else_.as_ref().clone()),
                _ => StepResult::Stuck,
            }
        }

        // Unit is a value
        Term::Unit => StepResult::Value,

        // Absurd
        Term::Absurd(ty, t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::absurd(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Stuck
        }

        // Zero is a value
        Term::Zero => StepResult::Value,

        // NatLit is a value (efficient representation for large natural numbers)
        Term::NatLit(_) => StepResult::Value,

        // Succ
        Term::Succ(t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::succ(t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // NatRec
        Term::NatRec(ty, zero_case, succ_case, n) => {
            if !n.is_value() {
                match step_with_env(n, env) {
                    StepResult::Stepped(n_new) => {
                        return StepResult::Stepped(Term::natrec(
                            ty.clone(),
                            zero_case.as_ref().clone(),
                            succ_case.as_ref().clone(),
                            n_new,
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match n.as_ref() {
                Term::Zero => StepResult::Stepped(zero_case.as_ref().clone()),
                Term::NatLit(0) => StepResult::Stepped(zero_case.as_ref().clone()),
                Term::NatLit(k) => {
                    let pred = Term::NatLit(k - 1);
                    let rec_call = Term::natrec(
                        ty.clone(),
                        zero_case.as_ref().clone(),
                        succ_case.as_ref().clone(),
                        pred.clone(),
                    );
                    let result = Term::app(Term::app(succ_case.as_ref().clone(), pred), rec_call);
                    StepResult::Stepped(result)
                }
                Term::Succ(pred) => {
                    let rec_call = Term::natrec(
                        ty.clone(),
                        zero_case.as_ref().clone(),
                        succ_case.as_ref().clone(),
                        pred.as_ref().clone(),
                    );
                    let result = Term::app(
                        Term::app(succ_case.as_ref().clone(), pred.as_ref().clone()),
                        rec_call,
                    );
                    StepResult::Stepped(result)
                }
                _ => StepResult::Stuck,
            }
        }

        // NatInd
        Term::NatInd(motive, zero_case, succ_case, n) => {
            if !n.is_value() {
                match step_with_env(n, env) {
                    StepResult::Stepped(n_new) => {
                        return StepResult::Stepped(Term::natind(
                            motive.clone(),
                            zero_case.as_ref().clone(),
                            succ_case.as_ref().clone(),
                            n_new,
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match n.as_ref() {
                Term::Zero => StepResult::Stepped(zero_case.as_ref().clone()),
                Term::NatLit(0) => StepResult::Stepped(zero_case.as_ref().clone()),
                Term::NatLit(k) => {
                    let pred = Term::NatLit(k - 1);
                    let rec_call = Term::natind(
                        motive.clone(),
                        zero_case.as_ref().clone(),
                        succ_case.as_ref().clone(),
                        pred.clone(),
                    );
                    let result = Term::app(Term::app(succ_case.as_ref().clone(), pred), rec_call);
                    StepResult::Stepped(result)
                }
                Term::Succ(pred) => {
                    let rec_call = Term::natind(
                        motive.clone(),
                        zero_case.as_ref().clone(),
                        succ_case.as_ref().clone(),
                        pred.as_ref().clone(),
                    );
                    let result = Term::app(
                        Term::app(succ_case.as_ref().clone(), pred.as_ref().clone()),
                        rec_call,
                    );
                    StepResult::Stepped(result)
                }
                _ => StepResult::Stuck,
            }
        }

        // Pair
        Term::Pair(t1, t2) => {
            if !t1.is_value() {
                match step_with_env(t1, env) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::pair(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !t2.is_value() {
                match step_with_env(t2, env) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::pair(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // Fst
        Term::Fst(t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::fst(t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match t.as_ref() {
                Term::Pair(v1, _) => StepResult::Stepped(v1.as_ref().clone()),
                _ => StepResult::Stuck,
            }
        }

        // Snd
        Term::Snd(t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::snd(t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match t.as_ref() {
                Term::Pair(_, v2) => StepResult::Stepped(v2.as_ref().clone()),
                _ => StepResult::Stuck,
            }
        }

        // Inl
        Term::Inl(ty, t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::inl(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // Inr
        Term::Inr(ty, t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::inr(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // Case
        Term::Case(scrut, x, left, y, right) => {
            if !scrut.is_value() {
                match step_with_env(scrut, env) {
                    StepResult::Stepped(scrut_new) => {
                        return StepResult::Stepped(Term::case(
                            scrut_new,
                            x.clone(),
                            left.as_ref().clone(),
                            y.clone(),
                            right.as_ref().clone(),
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match scrut.as_ref() {
                Term::Inl(_, v) => {
                    let result = left.substitute(x, v);
                    StepResult::Stepped(result)
                }
                Term::Inr(_, v) => {
                    let result = right.substitute(y, v);
                    StepResult::Stepped(result)
                }
                _ => StepResult::Stuck,
            }
        }

        // Type abstraction is a value
        Term::TyAbs(_, _) => StepResult::Value,

        // Type application
        Term::TyApp(t, _ty) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::ty_app(t_new, _ty.clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match t.as_ref() {
                Term::TyAbs(_, body) => StepResult::Stepped(body.as_ref().clone()),
                _ => StepResult::Stuck,
            }
        }

        // Refl
        Term::Refl(ty, t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::refl(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // Subst
        Term::Subst(ty, motive, eq_proof, proof) => {
            if !eq_proof.is_value() {
                match step_with_env(eq_proof, env) {
                    StepResult::Stepped(eq_new) => {
                        return StepResult::Stepped(Term::subst(
                            ty.clone(),
                            motive.clone(),
                            eq_new,
                            proof.as_ref().clone(),
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match eq_proof.as_ref() {
                Term::Refl(_, _) => StepResult::Stepped(proof.as_ref().clone()),
                _ => StepResult::Stuck,
            }
        }

        // Annot
        Term::Annot(t, ty) => {
            if t.is_value() {
                StepResult::Stepped(t.as_ref().clone())
            } else {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        StepResult::Stepped(Term::annot(t_new, ty.clone()))
                    }
                    other => other,
                }
            }
        }

        // Sorry
        Term::Sorry => StepResult::Stuck,

        // Phase 2A: Strings
        Term::StringLit(_) => StepResult::Value,

        Term::StrConcat(t1, t2) => {
            if !t1.is_value() {
                match step_with_env(t1, env) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::str_concat(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !t2.is_value() {
                match step_with_env(t2, env) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::str_concat(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (t1.as_ref(), t2.as_ref()) {
                (Term::StringLit(s1), Term::StringLit(s2)) => {
                    StepResult::Stepped(Term::string_lit(format!("{s1}{s2}")))
                }
                _ => StepResult::Stuck,
            }
        }

        Term::StrLen(t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::str_len(t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match t.as_ref() {
                Term::StringLit(s) => {
                    let len = s.len();
                    StepResult::Stepped(nat_to_term(len))
                }
                _ => StepResult::Stuck,
            }
        }

        Term::StrEq(t1, t2) => {
            if !t1.is_value() {
                match step_with_env(t1, env) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::str_eq(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !t2.is_value() {
                match step_with_env(t2, env) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::str_eq(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (t1.as_ref(), t2.as_ref()) {
                (Term::StringLit(s1), Term::StringLit(s2)) => {
                    if s1 == s2 {
                        StepResult::Stepped(Term::True)
                    } else {
                        StepResult::Stepped(Term::False)
                    }
                }
                _ => StepResult::Stuck,
            }
        }

        Term::StrCharAt(s, n) => {
            if !s.is_value() {
                match step_with_env(s, env) {
                    StepResult::Stepped(s_new) => {
                        return StepResult::Stepped(Term::str_char_at(s_new, n.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !n.is_value() {
                match step_with_env(n, env) {
                    StepResult::Stepped(n_new) => {
                        return StepResult::Stepped(Term::str_char_at(s.as_ref().clone(), n_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (s.as_ref(), term_to_nat(n)) {
                (Term::StringLit(str_val), Some(idx)) => {
                    if let Some(ch) = str_val.chars().nth(idx) {
                        StepResult::Stepped(nat_to_term(ch as usize))
                    } else {
                        StepResult::Stepped(Term::Zero) // Out of bounds → 0
                    }
                }
                _ => StepResult::Stuck,
            }
        }

        Term::StrSubstring(s, start, len) => {
            if !s.is_value() {
                match step_with_env(s, env) {
                    StepResult::Stepped(s_new) => {
                        return StepResult::Stepped(Term::str_substring(
                            s_new,
                            start.as_ref().clone(),
                            len.as_ref().clone(),
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !start.is_value() {
                match step_with_env(start, env) {
                    StepResult::Stepped(start_new) => {
                        return StepResult::Stepped(Term::str_substring(
                            s.as_ref().clone(),
                            start_new,
                            len.as_ref().clone(),
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !len.is_value() {
                match step_with_env(len, env) {
                    StepResult::Stepped(len_new) => {
                        return StepResult::Stepped(Term::str_substring(
                            s.as_ref().clone(),
                            start.as_ref().clone(),
                            len_new,
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (s.as_ref(), term_to_nat(start), term_to_nat(len)) {
                (Term::StringLit(str_val), Some(start_idx), Some(length)) => {
                    let chars: Vec<char> = str_val.chars().collect();
                    let result: String = chars.iter().skip(start_idx).take(length).collect();
                    StepResult::Stepped(Term::string_lit(result))
                }
                _ => StepResult::Stuck,
            }
        }

        // Phase 2A: Fix
        Term::Fix(f, ty, body) => {
            let unfolded =
                body.substitute(f, &Term::fix(f.clone(), ty.clone(), body.as_ref().clone()));
            StepResult::Stepped(unfolded)
        }

        // Phase 2A: Fold/Unfold
        Term::Fold(ty, t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::fold(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        Term::Unfold(ty, t) => {
            if !t.is_value() {
                match step_with_env(t, env) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::unfold(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match t.as_ref() {
                Term::Fold(_, inner) => StepResult::Stepped(inner.as_ref().clone()),
                _ => StepResult::Stuck,
            }
        }

        // Phase 3-Prep: Nat comparisons
        Term::NatLt(a, b) => {
            if !a.is_value() {
                match step_with_env(a, env) {
                    StepResult::Stepped(a_new) => {
                        return StepResult::Stepped(Term::nat_lt(a_new, b.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !b.is_value() {
                match step_with_env(b, env) {
                    StepResult::Stepped(b_new) => {
                        return StepResult::Stepped(Term::nat_lt(a.as_ref().clone(), b_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (term_to_nat(a), term_to_nat(b)) {
                (Some(a_val), Some(b_val)) => {
                    if a_val < b_val {
                        StepResult::Stepped(Term::True)
                    } else {
                        StepResult::Stepped(Term::False)
                    }
                }
                _ => StepResult::Stuck,
            }
        }

        Term::NatLe(a, b) => {
            if !a.is_value() {
                match step_with_env(a, env) {
                    StepResult::Stepped(a_new) => {
                        return StepResult::Stepped(Term::nat_le(a_new, b.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !b.is_value() {
                match step_with_env(b, env) {
                    StepResult::Stepped(b_new) => {
                        return StepResult::Stepped(Term::nat_le(a.as_ref().clone(), b_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (term_to_nat(a), term_to_nat(b)) {
                (Some(a_val), Some(b_val)) => {
                    if a_val <= b_val {
                        StepResult::Stepped(Term::True)
                    } else {
                        StepResult::Stepped(Term::False)
                    }
                }
                _ => StepResult::Stuck,
            }
        }

        Term::NatGt(a, b) => {
            if !a.is_value() {
                match step_with_env(a, env) {
                    StepResult::Stepped(a_new) => {
                        return StepResult::Stepped(Term::nat_gt(a_new, b.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !b.is_value() {
                match step_with_env(b, env) {
                    StepResult::Stepped(b_new) => {
                        return StepResult::Stepped(Term::nat_gt(a.as_ref().clone(), b_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (term_to_nat(a), term_to_nat(b)) {
                (Some(a_val), Some(b_val)) => {
                    if a_val > b_val {
                        StepResult::Stepped(Term::True)
                    } else {
                        StepResult::Stepped(Term::False)
                    }
                }
                _ => StepResult::Stuck,
            }
        }

        Term::NatGe(a, b) => {
            if !a.is_value() {
                match step_with_env(a, env) {
                    StepResult::Stepped(a_new) => {
                        return StepResult::Stepped(Term::nat_ge(a_new, b.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !b.is_value() {
                match step_with_env(b, env) {
                    StepResult::Stepped(b_new) => {
                        return StepResult::Stepped(Term::nat_ge(a.as_ref().clone(), b_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match (term_to_nat(a), term_to_nat(b)) {
                (Some(a_val), Some(b_val)) => {
                    if a_val >= b_val {
                        StepResult::Stepped(Term::True)
                    } else {
                        StepResult::Stepped(Term::False)
                    }
                }
                _ => StepResult::Stuck,
            }
        }

        // Phase 3-Prep: ExternCall (stuck in pure evaluation)
        Term::ExternCall(_, _) => StepResult::Stuck,

        // Phase 3-Prep: Ref cells (stuck in pure evaluation)
        Term::RefNew(_) | Term::RefGet(_) | Term::RefSet(_, _) => StepResult::Stuck,

        // Arithmetic operations (delegate to helpers)
        Term::NatAdd(a, b) => step_binary_nat_env(a, b, usize::saturating_add, Term::nat_add, env),
        Term::NatSub(a, b) => step_binary_nat_env(a, b, usize::saturating_sub, Term::nat_sub, env),
        Term::NatMul(a, b) => step_binary_nat_env(a, b, usize::saturating_mul, Term::nat_mul, env),
        Term::NatDiv(a, b) => step_binary_nat_env(
            a,
            b,
            |x, y| if y == 0 { 0 } else { x / y },
            Term::nat_div,
            env,
        ),
        Term::NatMod(a, b) => step_binary_nat_env(
            a,
            b,
            |x, y| if y == 0 { 0 } else { x % y },
            Term::nat_mod,
            env,
        ),
        Term::NatEq(a, b) => step_binary_nat_to_bool_env(a, b, |x, y| x == y, Term::nat_eq, env),
        Term::BoolAnd(a, b) => step_binary_bool_env(a, b, |x, y| x && y, Term::bool_and, env),
        Term::BoolOr(a, b) => step_binary_bool_env(a, b, |x, y| x || y, Term::bool_or, env),
        Term::BoolNot(a) => step_unary_bool_env(a, |x| !x, Term::bool_not, env),

        // Phase 2B: Flat ADT (ADR 2.2.26)

        // AdtConstruct: evaluate payload argument
        Term::AdtConstruct(adt_ty, idx, payload) => {
            if !payload.is_value() {
                match step_with_env(payload, env) {
                    StepResult::Stepped(payload_new) => {
                        return StepResult::Stepped(Term::adt_construct(
                            adt_ty.clone(),
                            *idx,
                            payload_new,
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value // adt_construct [adt_ty] idx v is a value
        }

        // AdtMatch: evaluate scrutinee, then dispatch to matching arm
        Term::AdtMatch(scrut, arms) => {
            if !scrut.is_value() {
                match step_with_env(scrut, env) {
                    StepResult::Stepped(scrut_new) => {
                        return StepResult::Stepped(Term::adt_match(scrut_new, arms.clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            // Scrutinee is a value: find matching arm by tag
            match scrut.as_ref() {
                Term::AdtConstruct(_, tag, payload) => {
                    // Find arm with matching tag
                    for (arm_idx, var, body) in arms {
                        if arm_idx == tag {
                            // Substitute payload for the bound variable
                            let result = body.substitute(var, payload);
                            return StepResult::Stepped(result);
                        }
                    }
                    // No matching arm found (incomplete match)
                    StepResult::Stuck
                }
                _ => StepResult::Stuck, // Not an ADT value
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    #[test]
    fn test_eval_env_empty() {
        let env = EvalEnv::empty();
        assert!(env.lookup("foo").is_none());
    }

    #[test]
    fn test_eval_env_lookup() {
        let mut globals = HashMap::new();
        globals.insert("x".to_string(), Term::Zero);
        let env = EvalEnv::new(globals);

        assert_eq!(env.lookup("x"), Some(Term::Zero));
        assert!(env.lookup("y").is_none());
    }

    #[test]
    fn test_global_lookup() {
        let mut globals = HashMap::new();
        globals.insert("x".to_string(), Term::Zero);
        let env = EvalEnv::new(globals);

        let result = eval_with_env(&Term::Global("x".into()), &env);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_global_undefined_stuck() {
        let env = EvalEnv::empty();
        let result = step_with_env(&Term::Global("undefined".into()), &env);
        assert_eq!(result, StepResult::Stuck);
    }

    #[test]
    fn test_call_by_need_memoization() {
        // Create an environment where looking up "x" returns an expression
        // that requires evaluation
        let mut globals = HashMap::new();
        globals.insert(
            "x".to_string(),
            Term::app(Term::lambda("y", Type::Nat, Term::var("y")), Term::Zero),
        );
        let env = EvalEnv::new(globals);

        // First lookup should evaluate and cache
        let result1 = env.lookup("x");
        assert_eq!(result1, Some(Term::Zero));

        // Second lookup should return cached value
        let result2 = env.lookup("x");
        assert_eq!(result2, Some(Term::Zero));

        // Verify it's actually cached
        assert!(env.cache.borrow().contains_key("x"));
    }

    #[test]
    fn test_nested_global_references() {
        // x = zero
        // y = x
        // main = y
        let mut globals = HashMap::new();
        globals.insert("x".to_string(), Term::Zero);
        globals.insert("y".to_string(), Term::Global("x".into()));
        globals.insert("main".to_string(), Term::Global("y".into()));
        let env = EvalEnv::new(globals);

        let result = eval_with_env(&Term::Global("main".into()), &env);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_global_in_application() {
        // id = λx:Nat. x
        // main = id zero
        let mut globals = HashMap::new();
        globals.insert(
            "id".to_string(),
            Term::lambda("x", Type::Nat, Term::var("x")),
        );
        let env = EvalEnv::new(globals);

        let term = Term::app(Term::Global("id".into()), Term::Zero);
        let result = eval_with_env(&term, &env);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_eval_with_env_and_limit_terminates() {
        let env = EvalEnv::empty();
        let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
        let result = eval_with_env_and_limit(&term, &env, 100);
        assert_eq!(result, Some(Term::Zero));
    }

    #[test]
    fn test_step_with_env_basic() {
        let env = EvalEnv::empty();

        // Lambda is a value
        assert_eq!(
            step_with_env(&Term::lambda("x", Type::Nat, Term::var("x")), &env),
            StepResult::Value
        );

        // Zero is a value
        assert_eq!(step_with_env(&Term::Zero, &env), StepResult::Value);

        // Variable is stuck
        assert_eq!(step_with_env(&Term::var("x"), &env), StepResult::Stuck);
    }
}
