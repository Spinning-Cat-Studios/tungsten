//! Intraprocedural escape analysis for μ-type fold operations.
//!
//! Identifies `Fold` allocations whose results never escape the current
//! function, allowing codegen to use stack allocation (`alloca`) instead
//! of heap allocation (`malloc`).
//!
//! A Fold is non-escaping when its result is let-bound and the binding
//! is only used in `Unfold` positions within the same let-body.
//!
//! See ADR 8.5.26d for design rationale.

use std::collections::HashSet;
use tungsten_core::terms::{Term, Var};

/// Result of escape analysis for a single definition.
///
/// Contains the set of let-bindings whose `Fold` values are non-escaping
/// and can be stack-allocated.
#[derive(Debug, Default)]
pub struct EscapeAnalysisResult {
    /// Variables bound to non-escaping Fold results.
    /// During codegen, when compiling a `Let(x, _, Fold(..), body)` where
    /// `x` is in this set, the Fold can use alloca instead of malloc.
    pub non_escaping_folds: HashSet<String>,
}

/// Run escape analysis on a term.
///
/// Identifies let-bound Fold results that are only used in Unfold positions
/// within the same let-body (and therefore never escape to the heap).
#[must_use]
pub fn analyze_escapes(term: &Term) -> EscapeAnalysisResult {
    let mut result = EscapeAnalysisResult::default();
    find_non_escaping_folds(term, &mut result);
    result
}

/// Walk the term tree looking for `Let(x, _, Fold(..), body)` patterns
/// where `x` is only used in `Unfold(_, Var(x))` within `body`.
fn find_non_escaping_folds(term: &Term, result: &mut EscapeAnalysisResult) {
    match term {
        Term::Let(x, _ty, def, body) => {
            // Check if the definition is a Fold
            if is_fold(def) {
                // Check if `x` only appears in Unfold positions within body
                if var_only_in_unfold(x, body) {
                    result.non_escaping_folds.insert(x.clone());
                }
            }
            // Continue analyzing nested terms
            find_non_escaping_folds(def, result);
            find_non_escaping_folds(body, result);
        }
        _ => {
            term.for_each_subterm(|child| find_non_escaping_folds(child, result));
        }
    }
}

/// Check if the outermost constructor of a term is `Fold`.
/// Also handles `Spanned` wrappers transparently.
fn is_fold(term: &Term) -> bool {
    match term {
        Term::Fold(_, _) => true,
        Term::Spanned(inner, _) => is_fold(inner),
        _ => false,
    }
}

/// Check whether variable `x` appears in `term` only inside `Unfold(_, Var(x))`.
///
/// Returns `true` if every occurrence of `x` in `term` is the immediate
/// argument of an `Unfold`. Returns `true` (vacuously) if `x` does not
/// appear at all.
///
/// This is the key safety check: if `x` is only unfolded, the pointer
/// never escapes (unfold just dereferences it to read the inner value).
fn var_only_in_unfold(x: &Var, term: &Term) -> bool {
    match term {
        // The critical case: Unfold with Var(x) as argument — this is safe,
        // the variable is used but only to read through the pointer.
        Term::Unfold(_, inner) => {
            if is_var(inner, x) {
                true
            } else {
                var_only_in_unfold(x, inner)
            }
        }
        // Direct use of x NOT inside an Unfold — escapes!
        Term::Var(v) => v != x,

        // Binding forms: if the inner binding shadows x, stop checking
        Term::Let(v, _, def, body) => {
            let def_safe = var_only_in_unfold(x, def);
            let body_safe = if v == x {
                true
            } else {
                var_only_in_unfold(x, body)
            };
            def_safe && body_safe
        }
        Term::Lambda(v, _, body) | Term::Fix(v, _, body) => {
            if v == x {
                true
            } else {
                // If x appears anywhere inside a lambda/fix body, it will be
                // captured in the closure environment. The closure can outlive the
                // stack frame, making the alloca pointer dangling. Conservatively
                // treat any occurrence inside a lambda as escaping.
                !term_contains_var(x, body)
            }
        }

        // Case/Match: binding forms with shadowing
        Term::Case(scrut, v1, left, v2, right) => {
            var_only_in_unfold(x, scrut)
                && (v1 == x || var_only_in_unfold(x, left))
                && (v2 == x || var_only_in_unfold(x, right))
        }
        Term::AdtMatch(scrut, arms) => {
            var_only_in_unfold(x, scrut)
                && arms
                    .iter()
                    .all(|(_, v, body)| v == x || var_only_in_unfold(x, body))
        }

        // All other terms: recurse into children
        _ => {
            let mut safe = true;
            term.for_each_subterm(|child| {
                safe = safe && var_only_in_unfold(x, child);
            });
            safe
        }
    }
}

/// Check if variable `x` appears anywhere in `term` (free occurrence).
/// Used to detect closure capture — any free occurrence of x inside a lambda
/// means x is captured and the pointer could escape.
fn term_contains_var(x: &Var, term: &Term) -> bool {
    match term {
        Term::Var(v) => v == x,
        Term::Let(v, _, def, body) => {
            term_contains_var(x, def) || (v != x && term_contains_var(x, body))
        }
        Term::Lambda(v, _, body) | Term::Fix(v, _, body) => v != x && term_contains_var(x, body),
        Term::Case(scrut, v1, left, v2, right) => {
            term_contains_var(x, scrut)
                || (v1 != x && term_contains_var(x, left))
                || (v2 != x && term_contains_var(x, right))
        }
        Term::AdtMatch(scrut, arms) => {
            term_contains_var(x, scrut)
                || arms
                    .iter()
                    .any(|(_, v, body)| v != x && term_contains_var(x, body))
        }
        _ => {
            let mut found = false;
            term.for_each_subterm(|child| {
                found = found || term_contains_var(x, child);
            });
            found
        }
    }
}

/// Check if a term is exactly `Var(x)`, looking through Spanned wrappers.
fn is_var(term: &Term, x: &Var) -> bool {
    match term {
        Term::Var(v) => v == x,
        Term::Spanned(inner, _) => is_var(inner, x),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_core::types::Type;

    fn var(name: &str) -> Term {
        Term::Var(name.to_string())
    }

    fn fold(inner: Term) -> Term {
        Term::Fold(Type::Unit, Box::new(inner))
    }

    fn unfold(inner: Term) -> Term {
        Term::Unfold(Type::Unit, Box::new(inner))
    }

    fn let_bind(name: &str, def: Term, body: Term) -> Term {
        Term::Let(name.to_string(), Type::Unit, Box::new(def), Box::new(body))
    }

    #[test]
    fn fold_immediately_unfolded_is_non_escaping() {
        // let x = fold(Unit) in unfold(x)
        let term = let_bind("x", fold(Term::Unit), unfold(var("x")));
        let result = analyze_escapes(&term);
        assert!(result.non_escaping_folds.contains("x"));
    }

    #[test]
    fn fold_used_in_app_is_escaping() {
        // let x = fold(Unit) in f(x)
        let term = let_bind(
            "x",
            fold(Term::Unit),
            Term::App(Box::new(var("f")), Box::new(var("x"))),
        );
        let result = analyze_escapes(&term);
        assert!(!result.non_escaping_folds.contains("x"));
    }

    #[test]
    fn fold_returned_is_escaping() {
        // let x = fold(Unit) in x
        let term = let_bind("x", fold(Term::Unit), var("x"));
        let result = analyze_escapes(&term);
        assert!(!result.non_escaping_folds.contains("x"));
    }

    #[test]
    fn fold_used_only_in_unfold_within_match_is_non_escaping() {
        // let x = fold(Unit) in match unfold(x) { ... }
        let term = let_bind(
            "x",
            fold(Term::Unit),
            Term::AdtMatch(
                Box::new(unfold(var("x"))),
                vec![(0, "payload".to_string(), Box::new(var("payload")))],
            ),
        );
        let result = analyze_escapes(&term);
        assert!(result.non_escaping_folds.contains("x"));
    }

    #[test]
    fn fold_stored_into_another_fold_is_escaping() {
        // let x = fold(Unit) in fold(x)  — x becomes part of a new heap value
        let term = let_bind("x", fold(Term::Unit), fold(var("x")));
        let result = analyze_escapes(&term);
        assert!(!result.non_escaping_folds.contains("x"));
    }

    #[test]
    fn non_fold_let_is_ignored() {
        // let x = Unit in unfold(x)
        let term = let_bind("x", Term::Unit, unfold(var("x")));
        let result = analyze_escapes(&term);
        assert!(!result.non_escaping_folds.contains("x"));
    }

    #[test]
    fn shadowed_variable_does_not_interfere() {
        // let x = fold(Unit) in let x = Zero in x
        let term = let_bind("x", fold(Term::Unit), let_bind("x", Term::Zero, var("x")));
        let result = analyze_escapes(&term);
        // The outer x is shadowed before use, so it's vacuously non-escaping
        assert!(result.non_escaping_folds.contains("x"));
    }

    #[test]
    fn fold_captured_in_lambda_is_escaping() {
        // let x = fold(Unit) in \y -> unfold(x)
        // Even though x is only used in unfold position, it's captured by the lambda
        // closure — the pointer would escape the stack frame.
        let term = let_bind(
            "x",
            fold(Term::Unit),
            Term::Lambda("y".to_string(), Type::Unit, Box::new(unfold(var("x")))),
        );
        let result = analyze_escapes(&term);
        assert!(!result.non_escaping_folds.contains("x"));
    }
}
