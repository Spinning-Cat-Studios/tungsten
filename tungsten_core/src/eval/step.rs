//! Standard small-step evaluator (without environment)
//!
//! This module provides the standard small-step call-by-value evaluator
//! that works on closed terms. Global references are stuck - for programs
//! with global definitions, use the environment-based evaluator instead.

use crate::terms::Term;

use super::helpers::{
    nat_to_term, step_binary_bool, step_binary_nat, step_binary_nat_compare, term_to_nat,
};
use super::StepResult;

// ============================================================================
// step - Standard small-step evaluator
// ============================================================================

/// Perform one step of call-by-value evaluation
///
/// This is the standard small-step evaluator that works on closed terms.
/// Global references are stuck; use [`super::env::step_with_env`] for
/// environment-based evaluation.
#[must_use]
pub fn step(term: &Term) -> StepResult {
    match term {
        // Variables are stuck (open term) or values depending on context
        Term::Var(_) => StepResult::Stuck,

        // Global references require environment-based evaluation (see step_with_env)
        // Without an environment, they are stuck
        Term::Global(_) => StepResult::Stuck,

        // Lambda is a value
        Term::Lambda(_, _, _) => StepResult::Value,

        // Application: evaluate to get function, then argument, then β-reduce
        Term::App(t1, t2) => {
            // If t1 is not a value, step it
            if !t1.is_value() {
                match step(t1) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::app(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {} // continue
                }
            }

            // If t1 is a value but t2 is not, step t2
            if !t2.is_value() {
                match step(t2) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::app(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {} // continue
                }
            }

            // Both are values: β-reduce
            match t1.as_ref() {
                Term::Lambda(x, _, body) => {
                    let result = body.substitute(x, t2);
                    StepResult::Stepped(result)
                }
                _ => StepResult::Stuck, // Not a function
            }
        }

        // Let: evaluate definition, then substitute
        Term::Let(x, _, def, body) => {
            if !def.is_value() {
                match step(def) {
                    StepResult::Stepped(def_new) => {
                        return StepResult::Stepped(Term::Let(
                            x.clone(),
                            term.let_type().unwrap().clone(),
                            Box::new(def_new),
                            body.clone(),
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            // Definition is a value, substitute
            StepResult::Stepped(body.substitute(x, def))
        }

        // Booleans are values
        Term::True | Term::False => StepResult::Value,

        // If: evaluate condition, then branch
        Term::If(cond, then_, else_) => {
            if !cond.is_value() {
                match step(cond) {
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

        // Absurd: evaluate argument (which will be stuck since Void has no values)
        Term::Absurd(ty, t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::absurd(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            // A value of type Void is impossible, so this is stuck
            StepResult::Stuck
        }

        // Zero is a value
        Term::Zero => StepResult::Value,

        // NatLit is a value (efficient representation for large natural numbers)
        Term::NatLit(_) => StepResult::Value,

        // Succ: evaluate argument
        Term::Succ(t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::succ(t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value // succ v is a value
        }

        // NatRec: primitive recursion
        Term::NatRec(ty, zero_case, succ_case, n) => {
            // Evaluate n first
            if !n.is_value() {
                match step(n) {
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
                    // natrec [τ] z s (NatLit k) → s (NatLit(k-1)) (natrec [τ] z s (NatLit(k-1)))
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
                    // natrec [τ] z s (succ v) → s v (natrec [τ] z s v)
                    let rec_call = Term::natrec(
                        ty.clone(),
                        zero_case.as_ref().clone(),
                        succ_case.as_ref().clone(),
                        pred.as_ref().clone(),
                    );
                    // s v (natrec ...)
                    let result = Term::app(
                        Term::app(succ_case.as_ref().clone(), pred.as_ref().clone()),
                        rec_call,
                    );
                    StepResult::Stepped(result)
                }
                _ => StepResult::Stuck,
            }
        }

        // NatInd: same semantics as NatRec (proofs compute the same way)
        Term::NatInd(motive, zero_case, succ_case, n) => {
            if !n.is_value() {
                match step(n) {
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

        // Pair: evaluate components
        Term::Pair(t1, t2) => {
            if !t1.is_value() {
                match step(t1) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::pair(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !t2.is_value() {
                match step(t2) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::pair(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // Fst: evaluate argument, then project
        Term::Fst(t) => {
            if !t.is_value() {
                match step(t) {
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

        // Snd: evaluate argument, then project
        Term::Snd(t) => {
            if !t.is_value() {
                match step(t) {
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

        // Inl: evaluate argument
        Term::Inl(ty, t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::inl(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // Inr: evaluate argument
        Term::Inr(ty, t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::inr(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value
        }

        // Case: evaluate scrutinee, then branch
        Term::Case(scrut, x, left, y, right) => {
            if !scrut.is_value() {
                match step(scrut) {
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

        // Type application: erase types at runtime
        Term::TyApp(t, _ty) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::ty_app(t_new, _ty.clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            match t.as_ref() {
                Term::TyAbs(_, body) => {
                    // Types are erased at runtime
                    StepResult::Stepped(body.as_ref().clone())
                }
                _ => StepResult::Stuck,
            }
        }

        // Refl: evaluate argument
        Term::Refl(ty, t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::refl(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value // refl [τ] v is a value
        }

        // Subst: if eq_proof is (refl v), return proof unchanged
        Term::Subst(ty, motive, eq_proof, proof) => {
            // Evaluate eq_proof first
            if !eq_proof.is_value() {
                match step(eq_proof) {
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

            // Evaluate proof
            if !proof.is_value() {
                match step(proof) {
                    StepResult::Stepped(proof_new) => {
                        return StepResult::Stepped(Term::subst(
                            ty.clone(),
                            motive.clone(),
                            eq_proof.as_ref().clone(),
                            proof_new,
                        ));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }

            // If eq_proof is refl, transport is identity
            match eq_proof.as_ref() {
                Term::Refl(_, _) => StepResult::Stepped(proof.as_ref().clone()),
                _ => StepResult::Stuck,
            }
        }

        // Annotation: evaluate inner term, annotation is erased
        Term::Annot(t, _) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(t_new);
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            // Once inner is a value, strip the annotation
            StepResult::Stepped(t.as_ref().clone())
        }

        // Sorry is stuck (never reduces)
        Term::Sorry => StepResult::Stuck,

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2A: Strings
        // ═══════════════════════════════════════════════════════════════════

        // String literals are values
        Term::StringLit(_) => StepResult::Value,

        // String concatenation
        Term::StrConcat(t1, t2) => {
            // Evaluate t1 first
            if !t1.is_value() {
                match step(t1) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::str_concat(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            // Then evaluate t2
            if !t2.is_value() {
                match step(t2) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::str_concat(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            // Both are values: concatenate
            match (t1.as_ref(), t2.as_ref()) {
                (Term::StringLit(s1), Term::StringLit(s2)) => {
                    StepResult::Stepped(Term::string_lit(format!("{s1}{s2}")))
                }
                _ => StepResult::Stuck,
            }
        }

        // String length
        Term::StrLen(t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::str_len(t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            match t.as_ref() {
                Term::StringLit(s) => {
                    // Return length as Nat (Succ^n(Zero))
                    StepResult::Stepped(Term::nat(s.len() as u64))
                }
                _ => StepResult::Stuck,
            }
        }

        // String equality
        Term::StrEq(t1, t2) => {
            // Evaluate t1 first
            if !t1.is_value() {
                match step(t1) {
                    StepResult::Stepped(t1_new) => {
                        return StepResult::Stepped(Term::str_eq(t1_new, t2.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            // Then evaluate t2
            if !t2.is_value() {
                match step(t2) {
                    StepResult::Stepped(t2_new) => {
                        return StepResult::Stepped(Term::str_eq(t1.as_ref().clone(), t2_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            // Both are values: compare
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

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2A: General Recursion
        // ═══════════════════════════════════════════════════════════════════

        // Fix: unfold one level
        // fix f:τ. t → t[f := fix f:τ. t]
        Term::Fix(f, ty, body) => {
            // Unfold the fixed point by substituting the fix expression for f
            let fix_term = Term::Fix(f.clone(), ty.clone(), body.clone());
            let result = body.substitute(f, &fix_term);
            StepResult::Stepped(result)
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2A: Recursive Types (μ-types)
        // ═══════════════════════════════════════════════════════════════════

        // Fold: evaluate argument
        Term::Fold(ty, t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::fold(ty.clone(), t_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            StepResult::Value // fold [μα.τ] v is a value
        }

        // Unfold: if argument is fold, extract inner value
        // unfold [μα.τ] (fold [μα.τ] v) → v
        Term::Unfold(ty, t) => {
            if !t.is_value() {
                match step(t) {
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

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3C: Arithmetic operations
        // ═══════════════════════════════════════════════════════════════════

        // NatAdd: a + b
        Term::NatAdd(t1, t2) => step_binary_nat(t1, t2, usize::saturating_add),

        // NatSub: a - b (saturating at 0)
        Term::NatSub(t1, t2) => step_binary_nat(t1, t2, usize::saturating_sub),

        // NatMul: a * b
        Term::NatMul(t1, t2) => step_binary_nat(t1, t2, usize::saturating_mul),

        // NatDiv: a / b
        Term::NatDiv(t1, t2) => step_binary_nat(t1, t2, |a, b| if b == 0 { 0 } else { a / b }),

        // NatMod: a % b
        Term::NatMod(t1, t2) => step_binary_nat(t1, t2, |a, b| if b == 0 { 0 } else { a % b }),

        // NatEq: a == b
        Term::NatEq(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a == b),

        // BoolAnd: a && b
        Term::BoolAnd(t1, t2) => step_binary_bool(t1, t2, |a, b| a && b),

        // BoolOr: a || b
        Term::BoolOr(t1, t2) => step_binary_bool(t1, t2, |a, b| a || b),

        // BoolNot: !a
        Term::BoolNot(t) => {
            if !t.is_value() {
                match step(t) {
                    StepResult::Stepped(t_new) => {
                        return StepResult::Stepped(Term::BoolNot(Box::new(t_new)));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            match t.as_ref() {
                Term::True => StepResult::Stepped(Term::False),
                Term::False => StepResult::Stepped(Term::True),
                _ => StepResult::Stuck,
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3-Prep: Comparison operations
        // ═══════════════════════════════════════════════════════════════════

        // NatLt: a < b
        Term::NatLt(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a < b),

        // NatLe: a <= b
        Term::NatLe(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a <= b),

        // NatGt: a > b
        Term::NatGt(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a > b),

        // NatGe: a >= b
        Term::NatGe(t1, t2) => step_binary_nat_compare(t1, t2, |a, b| a >= b),

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3-Prep: String character access
        // ═══════════════════════════════════════════════════════════════════

        // StrCharAt: get character at index, return ASCII code as Nat
        Term::StrCharAt(s, idx) => {
            if !s.is_value() {
                match step(s) {
                    StepResult::Stepped(s_new) => {
                        return StepResult::Stepped(Term::str_char_at(s_new, idx.as_ref().clone()));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            if !idx.is_value() {
                match step(idx) {
                    StepResult::Stepped(idx_new) => {
                        return StepResult::Stepped(Term::str_char_at(s.as_ref().clone(), idx_new));
                    }
                    StepResult::Stuck => return StepResult::Stuck,
                    StepResult::Value => {}
                }
            }
            // Both values: compute
            match (s.as_ref(), term_to_nat(idx)) {
                (Term::StringLit(str), Some(n)) => {
                    if n < str.len() {
                        let ch = str.as_bytes()[n];
                        StepResult::Stepped(nat_to_term(ch as usize))
                    } else {
                        StepResult::Stuck // Index out of bounds
                    }
                }
                _ => StepResult::Stuck,
            }
        }

        // StrSubstring: get substring starting at start with length len
        Term::StrSubstring(s, start, len) => {
            if !s.is_value() {
                match step(s) {
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
                match step(start) {
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
                match step(len) {
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
            // All values: compute
            match (s.as_ref(), term_to_nat(start), term_to_nat(len)) {
                (Term::StringLit(str), Some(start_idx), Some(length)) => {
                    let end_idx = (start_idx + length).min(str.len());
                    let start_idx = start_idx.min(str.len());
                    let result = &str[start_idx..end_idx];
                    StepResult::Stepped(Term::string_lit(result))
                }
                _ => StepResult::Stuck,
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // Phase 3-Prep: FFI and Ref cells
        // ═══════════════════════════════════════════════════════════════════

        // ExternCall: stuck in pure evaluation (needs codegen)
        Term::ExternCall(_, _) => StepResult::Stuck,

        // RefNew: stuck in pure evaluation (needs runtime)
        Term::RefNew(_) => StepResult::Stuck,

        // RefGet: stuck in pure evaluation (needs runtime)
        Term::RefGet(_) => StepResult::Stuck,

        // RefSet: stuck in pure evaluation (needs runtime)
        Term::RefSet(_, _) => StepResult::Stuck,

        // ═══════════════════════════════════════════════════════════════════
        // Phase 2B: Flat ADT (ADR 2.2.26)
        // ═══════════════════════════════════════════════════════════════════

        // AdtConstruct: evaluate payload argument
        Term::AdtConstruct(adt_ty, idx, payload) => {
            if !payload.is_value() {
                match step(payload) {
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
        // adt_match (adt_construct [T] idx v) [..., (idx, x, body), ...] → body[x := v]
        Term::AdtMatch(scrut, arms) => {
            if !scrut.is_value() {
                match step(scrut) {
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
    use crate::eval::{eval, eval_with_limit};
    use crate::types::Type;

    #[test]
    fn test_beta_reduction() {
        // (λx:Nat. x) zero → zero
        let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
        let result = eval(&term);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_if_true() {
        // if true then zero else succ zero → zero
        let term = Term::if_then_else(Term::True, Term::Zero, Term::succ(Term::Zero));
        let result = eval(&term);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_if_false() {
        // if false then zero else succ zero → succ zero
        let term = Term::if_then_else(Term::False, Term::Zero, Term::succ(Term::Zero));
        let result = eval(&term);
        assert_eq!(result, Term::succ(Term::Zero));
    }

    #[test]
    fn test_pair_fst() {
        // fst (zero, succ zero) → zero
        let term = Term::fst(Term::pair(Term::Zero, Term::succ(Term::Zero)));
        let result = eval(&term);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_pair_snd() {
        // snd (zero, succ zero) → succ zero
        let term = Term::snd(Term::pair(Term::Zero, Term::succ(Term::Zero)));
        let result = eval(&term);
        assert_eq!(result, Term::succ(Term::Zero));
    }

    #[test]
    fn test_let() {
        // let x : Nat = zero in succ x → succ zero
        let term = Term::let_in("x", Type::Nat, Term::Zero, Term::succ(Term::var("x")));
        let result = eval(&term);
        assert_eq!(result, Term::succ(Term::Zero));
    }

    #[test]
    fn test_natrec_zero() {
        // natrec [Nat] (succ zero) (λ_. λacc. acc) zero → succ zero
        let term = Term::natrec(
            Type::Nat,
            Term::succ(Term::Zero), // base case returns 1
            Term::lambda(
                "_",
                Type::Nat,
                Term::lambda("acc", Type::Nat, Term::var("acc")),
            ),
            Term::Zero,
        );
        let result = eval(&term);
        assert_eq!(result, Term::succ(Term::Zero));
    }

    #[test]
    fn test_natrec_succ() {
        // natrec [Nat] zero (λ_. λacc. succ acc) (succ (succ zero)) → 2
        // This computes the length essentially
        let term = Term::natrec(
            Type::Nat,
            Term::Zero,
            Term::lambda(
                "_",
                Type::Nat,
                Term::lambda("acc", Type::Nat, Term::succ(Term::var("acc"))),
            ),
            Term::succ(Term::succ(Term::Zero)),
        );
        let result = eval(&term);
        assert_eq!(result, Term::nat(2));
    }

    #[test]
    fn test_case_inl() {
        // case (inl zero : Nat + Bool) of inl x → succ x | inr _ → zero
        let sum_ty = Type::sum(Type::Nat, Type::Bool);
        let term = Term::case(
            Term::inl(sum_ty, Term::Zero),
            "x",
            Term::succ(Term::var("x")),
            "_",
            Term::Zero,
        );
        let result = eval(&term);
        assert_eq!(result, Term::succ(Term::Zero));
    }

    #[test]
    fn test_case_inr() {
        // case (inr true : Nat + Bool) of inl _ → false | inr b → b
        let sum_ty = Type::sum(Type::Nat, Type::Bool);
        let term = Term::case(
            Term::inr(sum_ty, Term::True),
            "_",
            Term::False,
            "b",
            Term::var("b"),
        );
        let result = eval(&term);
        assert_eq!(result, Term::True);
    }

    #[test]
    fn test_type_application() {
        // (Λα. λx:α. x) [Nat] zero → (λx:Nat. x) zero → zero
        let poly_id = Term::ty_abs(
            "α",
            Term::lambda("x", Type::TyVar("α".into()), Term::var("x")),
        );
        let term = Term::app(Term::ty_app(poly_id, Type::Nat), Term::Zero);
        let result = eval(&term);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_subst_refl() {
        // subst [Nat] [P] (refl [Nat] zero) proof → proof
        let motive = Type::arrow(Type::Nat, Type::Prop);
        let term = Term::subst(
            Type::Nat,
            motive,
            Term::refl(Type::Nat, Term::Zero),
            Term::Unit, // Using Unit as a dummy proof
        );
        let result = eval(&term);
        assert_eq!(result, Term::Unit);
    }

    #[test]
    fn test_nested_application() {
        // (λf. λx. f (f x)) (λn. succ n) zero → succ (succ zero)
        let double_apply = Term::lambda(
            "f",
            Type::arrow(Type::Nat, Type::Nat),
            Term::lambda(
                "x",
                Type::Nat,
                Term::app(Term::var("f"), Term::app(Term::var("f"), Term::var("x"))),
            ),
        );
        let succ_fn = Term::lambda("n", Type::Nat, Term::succ(Term::var("n")));
        let term = Term::app(Term::app(double_apply, succ_fn), Term::Zero);
        let result = eval(&term);
        assert_eq!(result, Term::nat(2));
    }

    #[test]
    fn test_sorry_stuck() {
        let result = step(&Term::Sorry);
        assert_eq!(result, StepResult::Stuck);
    }

    #[test]
    fn test_eval_with_limit() {
        // Normal termination
        let term = Term::app(Term::lambda("x", Type::Nat, Term::var("x")), Term::Zero);
        let result = eval_with_limit(&term, 100);
        assert_eq!(result, Some(Term::Zero));
    }

    // ==========================================================================
    // Phase 2A Tests: Strings
    // ==========================================================================

    #[test]
    fn test_string_literal_is_value() {
        let term = Term::string_lit("hello");
        let result = step(&term);
        assert_eq!(result, StepResult::Value);
        assert!(term.is_value());
    }

    #[test]
    fn test_str_concat() {
        let term = Term::str_concat(Term::string_lit("hello"), Term::string_lit(" world"));
        let result = eval(&term);
        assert_eq!(result, Term::string_lit("hello world"));
    }

    #[test]
    fn test_str_concat_empty() {
        let term = Term::str_concat(Term::string_lit(""), Term::string_lit("test"));
        let result = eval(&term);
        assert_eq!(result, Term::string_lit("test"));
    }

    #[test]
    fn test_str_len_empty() {
        let term = Term::str_len(Term::string_lit(""));
        let result = eval(&term);
        assert_eq!(result, Term::Zero);
    }

    #[test]
    fn test_str_len_nonempty() {
        let term = Term::str_len(Term::string_lit("hello"));
        let result = eval(&term);
        assert_eq!(result, Term::nat(5));
    }

    #[test]
    fn test_str_eq_true() {
        let term = Term::str_eq(Term::string_lit("hello"), Term::string_lit("hello"));
        let result = eval(&term);
        assert_eq!(result, Term::True);
    }

    #[test]
    fn test_str_eq_false() {
        let term = Term::str_eq(Term::string_lit("hello"), Term::string_lit("world"));
        let result = eval(&term);
        assert_eq!(result, Term::False);
    }

    #[test]
    fn test_str_concat_nested() {
        // ("a" ++ "b") ++ "c" → "abc"
        let term = Term::str_concat(
            Term::str_concat(Term::string_lit("a"), Term::string_lit("b")),
            Term::string_lit("c"),
        );
        let result = eval(&term);
        assert_eq!(result, Term::string_lit("abc"));
    }

    // ==========================================================================
    // Phase 2A Tests: Fix Combinator
    // ==========================================================================

    #[test]
    fn test_fix_identity() {
        // fix f:(Nat → Nat). λn:Nat. n   applied to 5 → 5
        // This is a trivial recursion that just returns the argument
        let fix_term = Term::fix(
            "f",
            Type::arrow(Type::Nat, Type::Nat),
            Term::lambda("n", Type::Nat, Term::var("n")),
        );
        let term = Term::app(fix_term, Term::nat(5));
        let result = eval(&term);
        assert_eq!(result, Term::nat(5));
    }

    #[test]
    fn test_fix_double() {
        // fix f:(Nat → Nat). λn:Nat. natrec<Nat>(n, succ (succ n), λ_:Nat.λacc:Nat. acc)
        // This computes n + n = 2n using natrec
        // Actually let's just do a simpler test: fix that adds 1
        let fix_term = Term::fix(
            "f",
            Type::arrow(Type::Nat, Type::Nat),
            Term::lambda("n", Type::Nat, Term::succ(Term::var("n"))),
        );
        let term = Term::app(fix_term, Term::nat(3));
        let result = eval(&term);
        assert_eq!(result, Term::nat(4));
    }

    // ==========================================================================
    // Phase 2A Tests: μ-types (Iso-recursive)
    // ==========================================================================

    #[test]
    fn test_fold_is_value() {
        // fold [μα. 1 + α] (inl unit) is a value
        let mu_ty = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
        let term = Term::fold(
            mu_ty.clone(),
            Term::inl(Type::sum(Type::Unit, mu_ty), Term::Unit),
        );
        assert!(term.is_value());
        let result = step(&term);
        assert_eq!(result, StepResult::Value);
    }

    #[test]
    fn test_unfold_fold() {
        // unfold [μα. τ] (fold [μα. τ] v) → v
        let mu_ty = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
        let inner = Term::inl(Type::sum(Type::Unit, mu_ty.clone()), Term::Unit);
        let folded = Term::fold(mu_ty.clone(), inner.clone());
        let term = Term::unfold(mu_ty, folded);
        let result = eval(&term);
        assert_eq!(result, inner);
    }

    #[test]
    fn test_mu_peano_zero() {
        // Represent Peano naturals as μ-type: μα. 1 + α
        // Zero = fold (inl ())
        let peano = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
        let unfolded_ty = Type::sum(Type::Unit, peano.clone());
        let zero = Term::fold(peano.clone(), Term::inl(unfolded_ty.clone(), Term::Unit));

        // Unfolding zero gives inl ()
        let unfolded = Term::unfold(peano, zero);
        let result = eval(&unfolded);

        // Check it's a left injection
        match result {
            Term::Inl(_, inner) => assert_eq!(*inner, Term::Unit),
            _ => panic!("Expected Inl"),
        }
    }

    #[test]
    fn test_mu_peano_succ() {
        // Succ(Zero) = fold (inr (fold (inl ())))
        let peano = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
        let unfolded_ty = Type::sum(Type::Unit, peano.clone());

        let zero = Term::fold(peano.clone(), Term::inl(unfolded_ty.clone(), Term::Unit));
        let one = Term::fold(peano.clone(), Term::inr(unfolded_ty.clone(), zero));

        // Unfolding one gives inr (fold (inl ()))
        let unfolded = Term::unfold(peano.clone(), one);
        let result = eval(&unfolded);

        // Check it's a right injection containing zero
        match result {
            Term::Inr(_, inner) => {
                // inner should be fold (inl ())
                match inner.as_ref() {
                    Term::Fold(_, inner_inner) => match inner_inner.as_ref() {
                        Term::Inl(_, u) => assert_eq!(u.as_ref(), &Term::Unit),
                        _ => panic!("Expected Inl inside Fold"),
                    },
                    _ => panic!("Expected Fold inside Inr"),
                }
            }
            _ => panic!("Expected Inr"),
        }
    }

    // ==========================================================================
    // Phase 3-Prep Tests: Integer Comparison
    // ==========================================================================

    #[test]
    fn test_nat_lt_true() {
        // 2 < 5 → true
        let term = Term::nat_lt(Term::nat(2), Term::nat(5));
        let result = eval(&term);
        assert_eq!(result, Term::True);
    }

    #[test]
    fn test_nat_lt_false() {
        // 5 < 2 → false
        let term = Term::nat_lt(Term::nat(5), Term::nat(2));
        let result = eval(&term);
        assert_eq!(result, Term::False);
    }

    #[test]
    fn test_nat_lt_equal() {
        // 3 < 3 → false
        let term = Term::nat_lt(Term::nat(3), Term::nat(3));
        let result = eval(&term);
        assert_eq!(result, Term::False);
    }

    #[test]
    fn test_nat_le_true() {
        // 3 <= 3 → true
        let term = Term::nat_le(Term::nat(3), Term::nat(3));
        let result = eval(&term);
        assert_eq!(result, Term::True);
    }

    #[test]
    fn test_nat_le_false() {
        // 5 <= 3 → false
        let term = Term::nat_le(Term::nat(5), Term::nat(3));
        let result = eval(&term);
        assert_eq!(result, Term::False);
    }

    #[test]
    fn test_nat_gt_true() {
        // 5 > 3 → true
        let term = Term::nat_gt(Term::nat(5), Term::nat(3));
        let result = eval(&term);
        assert_eq!(result, Term::True);
    }

    #[test]
    fn test_nat_gt_false() {
        // 3 > 5 → false
        let term = Term::nat_gt(Term::nat(3), Term::nat(5));
        let result = eval(&term);
        assert_eq!(result, Term::False);
    }

    #[test]
    fn test_nat_ge_true() {
        // 5 >= 5 → true
        let term = Term::nat_ge(Term::nat(5), Term::nat(5));
        let result = eval(&term);
        assert_eq!(result, Term::True);
    }

    #[test]
    fn test_nat_ge_false() {
        // 3 >= 5 → false
        let term = Term::nat_ge(Term::nat(3), Term::nat(5));
        let result = eval(&term);
        assert_eq!(result, Term::False);
    }

    // ==========================================================================
    // Phase 3-Prep Tests: String char_at
    // ==========================================================================

    #[test]
    fn test_str_char_at() {
        // char_at "hello" 0 → 104 ('h')
        let term = Term::str_char_at(Term::string_lit("hello"), Term::Zero);
        let result = eval(&term);
        assert_eq!(result, Term::nat(104)); // ASCII 'h' = 104
    }

    #[test]
    fn test_str_char_at_middle() {
        // char_at "hello" 2 → 108 ('l')
        let term = Term::str_char_at(Term::string_lit("hello"), Term::nat(2));
        let result = eval(&term);
        assert_eq!(result, Term::nat(108)); // ASCII 'l' = 108
    }

    #[test]
    fn test_str_char_at_out_of_bounds() {
        // char_at "hi" 5 → stuck (out of bounds)
        let term = Term::str_char_at(Term::string_lit("hi"), Term::nat(5));
        let result = step(&term);
        assert_eq!(result, StepResult::Stuck);
    }

    // ==========================================================================
    // Phase 3-Prep Tests: FFI and Ref Cells (stuck in pure evaluation)
    // ==========================================================================

    #[test]
    fn test_extern_call_stuck() {
        // extern_call "puts" ["hello"] → stuck (needs codegen)
        let term = Term::extern_call("puts", vec![Term::string_lit("hello")]);
        let result = step(&term);
        assert_eq!(result, StepResult::Stuck);
    }

    #[test]
    fn test_ref_new_stuck() {
        // ref 42 → stuck (needs runtime)
        let term = Term::ref_new(Term::nat(42));
        let result = step(&term);
        assert_eq!(result, StepResult::Stuck);
    }

    #[test]
    fn test_ref_get_stuck() {
        // get r → stuck (needs runtime)
        let term = Term::ref_get(Term::var("r"));
        let result = step(&term);
        assert_eq!(result, StepResult::Stuck);
    }

    #[test]
    fn test_ref_set_stuck() {
        // set r 42 → stuck (needs runtime)
        let term = Term::ref_set(Term::var("r"), Term::nat(42));
        let result = step(&term);
        assert_eq!(result, StepResult::Stuck);
    }
}
