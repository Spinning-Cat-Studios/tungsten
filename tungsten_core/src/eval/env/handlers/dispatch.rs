//! Sub-dispatchers for the multi-variant `Term` families.
//!
//! `step_with_env` fans out to one arm per family — core lambda calculus,
//! strings, arithmetic/boolean, and the `Return` wrapper. They live here rather
//! than beside the dispatcher so `env/mod.rs` stays under the size limit and so
//! each family reads as one table.

use crate::eval::env::helpers::{
    step_binary_bool_env, step_binary_nat_env, step_binary_nat_to_bool_env, step_nat_compare_env,
    step_unary_bool_env,
};
use crate::eval::env::{step_with_env, EvalEnv};
use crate::eval::StepResult;
use crate::terms::Term;

use super::{
    step_annot_env, step_app_env, step_case_env, step_fst_env, step_if_env, step_let_env,
    step_natind_env, step_natrec_env, step_pair_env, step_snd_env, step_str_char_at_env,
    step_str_concat_env, step_str_eq_env, step_str_len_env, step_str_substring_env, step_subst_env,
    step_tyapp_env, step_unfold_env,
};

/// Step a Return term with environment: strip the wrapper when the inner value is ready.
pub(in crate::eval::env) fn step_return_env(t: &Term, env: &EvalEnv) -> StepResult {
    if t.is_value() {
        return StepResult::Stepped(t.clone());
    }
    match step_with_env(t, env) {
        StepResult::Stepped(t_new) => StepResult::Stepped(Term::early_return(t_new)),
        other => other,
    }
}

/// Core term forms with environment: application, let, if, fix, products, sums, proof/recursion.
pub(in crate::eval::env) fn step_core_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        Term::App(t1, t2) => step_app_env(t1, t2, env),
        Term::Let(x, ty, def, body) => step_let_env(x, ty, def, body, env),
        Term::If(cond, then_, else_) => step_if_env(cond, then_, else_, env),
        Term::TyApp(t, ty) => step_tyapp_env(t, ty, env),
        Term::Annot(t, ty) => step_annot_env(t, ty, env),
        Term::Fix(f, ty, body) => {
            let unfolded =
                body.substitute(f, &Term::fix(f.clone(), ty.clone(), body.as_ref().clone()));
            StepResult::Stepped(unfolded)
        }
        Term::Pair(t1, t2) => step_pair_env(t1, t2, env),
        Term::Fst(t) => step_fst_env(t, env),
        Term::Snd(t) => step_snd_env(t, env),
        Term::Case(scrut, x, left, y, right) => {
            use super::CaseArm;
            step_case_env(
                scrut,
                &CaseArm { var: x, body: left },
                &CaseArm {
                    var: y,
                    body: right,
                },
                env,
            )
        }
        Term::NatRec(ty, z, s, n) => step_natrec_env(ty, z, s, n, env),
        Term::NatInd(m, z, s, n) => step_natind_env(m, z, s, n, env),
        Term::Subst(ty, motive, eq, proof) => step_subst_env(ty, motive, eq, proof, env),
        Term::Unfold(ty, t) => step_unfold_env(t, ty, env),
        _ => unreachable!("step_core_env called with non-core term"),
    }
}

/// String operation steps with environment.
pub(in crate::eval::env) fn step_string_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        Term::StrConcat(t1, t2) => step_str_concat_env(t1, t2, env),
        Term::StrLen(t) => step_str_len_env(t, env),
        Term::StrEq(t1, t2) => step_str_eq_env(t1, t2, env),
        Term::StrCharAt(s, n) => step_str_char_at_env(s, n, env),
        Term::StrSubstring(s, start, len) => step_str_substring_env(s, start, len, env),
        _ => unreachable!("step_string_env called with non-string term"),
    }
}

/// Arithmetic and boolean operation steps with environment.
pub(in crate::eval::env) fn step_arith_bool_env(term: &Term, env: &EvalEnv) -> StepResult {
    match term {
        Term::NatLt(a, b) => step_nat_compare_env(a, b, |x, y| x < y, Term::nat_lt, env),
        Term::NatLe(a, b) => step_nat_compare_env(a, b, |x, y| x <= y, Term::nat_le, env),
        Term::NatGt(a, b) => step_nat_compare_env(a, b, |x, y| x > y, Term::nat_gt, env),
        Term::NatGe(a, b) => step_nat_compare_env(a, b, |x, y| x >= y, Term::nat_ge, env),
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
        _ => unreachable!("step_arith_bool_env called with non-arith/bool term"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::env::eval_with_env;

    /// Evaluate a closed arithmetic term and read its value as a number.
    ///
    /// Results come back as either a `NatLit` or a Peano `Succ` chain
    /// depending on the operand shapes, so the assertions compare *numbers*
    /// rather than term spellings — a test that pinned the spelling would fail
    /// on a representation change that broke nothing.
    fn eval_nat(term: Term) -> u64 {
        let value = eval_with_env(&term, &EvalEnv::empty()).expect("closed arithmetic cannot stop");
        crate::eval::term_to_nat(&value).expect("an arithmetic term evaluates to a Nat") as u64
    }

    fn eval(term: Term) -> Term {
        eval_with_env(&term, &EvalEnv::empty()).expect("closed arithmetic cannot stop")
    }

    fn nat(n: u64) -> Term {
        Term::NatLit(n)
    }

    fn is_true(term: Term) -> bool {
        matches!(eval(term), Term::True)
    }

    /// Each comparison is asserted at all three orderings, because a mutant
    /// that swaps one relation for another agrees with the original on most
    /// operand pairs — `3 < 5` and `3 <= 5` differ only at equality, and
    /// `<` vs `>` differ nowhere a single one-sided test looks.
    #[test]
    fn nat_comparisons_distinguish_less_equal_and_greater() {
        for (build, lt, eq, gt) in [
            (Term::nat_lt as fn(Term, Term) -> Term, true, false, false),
            (Term::nat_le, true, true, false),
            (Term::nat_gt, false, false, true),
            (Term::nat_ge, false, true, true),
        ] {
            assert_eq!(is_true(build(nat(3), nat(5))), lt, "3 ? 5");
            assert_eq!(is_true(build(nat(4), nat(4))), eq, "4 ? 4");
            assert_eq!(is_true(build(nat(5), nat(3))), gt, "5 ? 3");
        }
    }

    #[test]
    fn nat_equality_is_not_disequality() {
        assert!(is_true(Term::nat_eq(nat(4), nat(4))));
        assert!(!is_true(Term::nat_eq(nat(4), nat(5))));
    }

    /// Operands chosen so each operator's result is unique among the four:
    /// 7 and 2 give 9, 5, 14, 3, 1 — no two agree, so swapping any pair of
    /// operators changes the answer.
    #[test]
    fn nat_arithmetic_operators_are_distinguishable() {
        assert_eq!(eval_nat(Term::nat_add(nat(7), nat(2))), 9);
        assert_eq!(eval_nat(Term::nat_sub(nat(7), nat(2))), 5);
        assert_eq!(eval_nat(Term::nat_mul(nat(7), nat(2))), 14);
        assert_eq!(eval_nat(Term::nat_div(nat(7), nat(2))), 3);
        assert_eq!(eval_nat(Term::nat_mod(nat(7), nat(2))), 1);
    }

    /// Division and modulo by zero yield zero rather than trapping — the
    /// evaluator has no exception, and a panicking arm would take the whole
    /// run down.
    #[test]
    fn division_and_modulo_by_zero_are_zero() {
        assert_eq!(eval_nat(Term::nat_div(nat(7), nat(0))), 0);
        assert_eq!(eval_nat(Term::nat_mod(nat(7), nat(0))), 0);
    }

    /// `and`/`or` are asserted where they DIFFER (one operand true, one
    /// false); an all-true or all-false table is satisfied by either.
    #[test]
    fn boolean_connectives_differ_on_mixed_operands() {
        assert!(!is_true(Term::bool_and(Term::True, Term::False)));
        assert!(is_true(Term::bool_or(Term::True, Term::False)));
        assert!(is_true(Term::bool_and(Term::True, Term::True)));
        assert!(!is_true(Term::bool_or(Term::False, Term::False)));
    }

    #[test]
    fn negation_inverts_rather_than_forwards() {
        assert!(is_true(Term::bool_not(Term::False)));
        assert!(!is_true(Term::bool_not(Term::True)));
    }
}
