//! Extension substitution handlers for Phase 2A/3C/3-Prep/2B terms.
//!
//! These are the mechanical substitution arms for strings, arithmetic,
//! booleans, refs, and ADTs — extracted from the main `substitute()`
//! and `substitute_type()` to reduce function size.

use crate::terms::Term;
use crate::types::Type;

/// Helper: substitute into both sub-terms of a binary operation.
fn sub_binop(
    t1: &Term,
    t2: &Term,
    var: &str,
    replacement: &Term,
    ctor: fn(Box<Term>, Box<Term>) -> Term,
) -> Term {
    ctor(
        Box::new(t1.substitute(var, replacement)),
        Box::new(t2.substitute(var, replacement)),
    )
}

/// For binary extension terms with no type annotations, extract sub-terms and constructor.
fn ext_binary_sub_terms(term: &Term) -> Option<(&Term, &Term, fn(Box<Term>, Box<Term>) -> Term)> {
    match term {
        Term::StrConcat(a, b) => Some((a, b, Term::StrConcat)),
        Term::StrEq(a, b) => Some((a, b, Term::StrEq)),
        Term::StrCharAt(a, b) => Some((a, b, Term::StrCharAt)),
        Term::NatAdd(a, b) => Some((a, b, Term::NatAdd)),
        Term::NatSub(a, b) => Some((a, b, Term::NatSub)),
        Term::NatMul(a, b) => Some((a, b, Term::NatMul)),
        Term::NatDiv(a, b) => Some((a, b, Term::NatDiv)),
        Term::NatMod(a, b) => Some((a, b, Term::NatMod)),
        Term::NatEq(a, b) => Some((a, b, Term::NatEq)),
        Term::NatLt(a, b) => Some((a, b, Term::NatLt)),
        Term::NatLe(a, b) => Some((a, b, Term::NatLe)),
        Term::NatGt(a, b) => Some((a, b, Term::NatGt)),
        Term::NatGe(a, b) => Some((a, b, Term::NatGe)),
        Term::BoolAnd(a, b) => Some((a, b, Term::BoolAnd)),
        Term::BoolOr(a, b) => Some((a, b, Term::BoolOr)),
        Term::RefSet(a, b) => Some((a, b, Term::RefSet)),
        _ => None,
    }
}

/// For unary extension terms with no type annotations, extract sub-term and constructor.
fn ext_unary_sub_term(term: &Term) -> Option<(&Term, fn(Term) -> Term)> {
    match term {
        Term::StrLen(t) => Some((t, Term::str_len)),
        Term::BoolNot(t) => Some((t, Term::bool_not)),
        Term::RefNew(t) => Some((t, Term::ref_new)),
        Term::RefGet(t) => Some((t, Term::ref_get)),
        _ => None,
    }
}

/// Term-variable substitution for extension term forms.
pub(super) fn substitute_ext(term: &Term, var: &str, replacement: &Term) -> Term {
    if let Some((t1, t2, ctor)) = ext_binary_sub_terms(term) {
        return sub_binop(t1, t2, var, replacement, ctor);
    }
    if let Some((t, ctor)) = ext_unary_sub_term(term) {
        return ctor(t.substitute(var, replacement));
    }
    match term {
        Term::StringLit(s) => Term::StringLit(s.clone()),

        Term::Fold(ty, t) => Term::fold(ty.clone(), t.substitute(var, replacement)),
        Term::Unfold(ty, t) => Term::unfold(ty.clone(), t.substitute(var, replacement)),

        Term::StrSubstring(s, start, len) => Term::str_substring(
            s.substitute(var, replacement),
            start.substitute(var, replacement),
            len.substitute(var, replacement),
        ),

        Term::ExternCall(name, args) => {
            let new_args = args
                .iter()
                .map(|a| a.substitute(var, replacement))
                .collect();
            Term::ExternCall(name.clone(), new_args)
        }

        Term::AdtConstruct(adt_ty, idx, payload) => {
            Term::adt_construct(adt_ty.clone(), *idx, payload.substitute(var, replacement))
        }
        Term::AdtMatch(scrut, arms) => {
            let new_scrut = scrut.substitute(var, replacement);
            let new_arms: Vec<_> = arms
                .iter()
                .map(|(idx, bound_var, body)| {
                    (
                        *idx,
                        bound_var.clone(),
                        Box::new(super::sub_if_unshadowed(body, bound_var, var, replacement)),
                    )
                })
                .collect();
            Term::adt_match(new_scrut, new_arms)
        }

        _ => unreachable!("unhandled term variant in substitute_ext"),
    }
}

// ============================================================================
// Type-variable substitution for extension terms
// ============================================================================

/// Helper: substitute types in both sub-terms of a binary operation.
fn sub_type_binop(
    t1: &Term,
    t2: &Term,
    var: &str,
    replacement: &Type,
    ctor: fn(Box<Term>, Box<Term>) -> Term,
) -> Term {
    ctor(
        Box::new(t1.substitute_type(var, replacement)),
        Box::new(t2.substitute_type(var, replacement)),
    )
}

/// Type-variable substitution for extension term forms.
pub(super) fn substitute_type_ext(term: &Term, var: &str, replacement: &Type) -> Term {
    if let Some((t1, t2, ctor)) = ext_binary_sub_terms(term) {
        return sub_type_binop(t1, t2, var, replacement, ctor);
    }
    if let Some((t, ctor)) = ext_unary_sub_term(term) {
        return ctor(t.substitute_type(var, replacement));
    }
    match term {
        Term::StringLit(s) => Term::StringLit(s.clone()),

        Term::Fold(ty, t) => Term::fold(
            ty.substitute(var, replacement),
            t.substitute_type(var, replacement),
        ),
        Term::Unfold(ty, t) => Term::unfold(
            ty.substitute(var, replacement),
            t.substitute_type(var, replacement),
        ),

        Term::StrSubstring(s, start, len) => Term::str_substring(
            s.substitute_type(var, replacement),
            start.substitute_type(var, replacement),
            len.substitute_type(var, replacement),
        ),

        Term::ExternCall(name, args) => {
            let new_args = args
                .iter()
                .map(|a| a.substitute_type(var, replacement))
                .collect();
            Term::ExternCall(name.clone(), new_args)
        }

        Term::AdtConstruct(adt_ty, idx, payload) => Term::adt_construct(
            adt_ty.substitute(var, replacement),
            *idx,
            payload.substitute_type(var, replacement),
        ),
        Term::AdtMatch(scrut, arms) => {
            let new_scrut = scrut.substitute_type(var, replacement);
            let new_arms: Vec<_> = arms
                .iter()
                .map(|(idx, bound_var, body)| {
                    (
                        *idx,
                        bound_var.clone(),
                        Box::new(body.substitute_type(var, replacement)),
                    )
                })
                .collect();
            Term::adt_match(new_scrut, new_arms)
        }

        _ => unreachable!("unhandled term variant in substitute_type_ext"),
    }
}
