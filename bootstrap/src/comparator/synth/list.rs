//! Cons-list `μ` comparator synthesis: the stack-safe tail-recursive spine
//! (ADR 29.6.26f P4).
//!
//! A cons-list `μα. NonRec + (Elem × α)` synthesizes **two** defs — the wrapper
//! `compare_List_T` (seeds the spine at index 0) and the spine `compare_List_T_spine`
//! whose "heads equal ⇒ recurse on tails" call is in tail position (musttail'd via
//! 1.7.26a's sret path → O(1) native stack for the 100 000-element AC). Other `μ`
//! shapes (e.g. trees) keep native recursion via `mu_comparator`.

use tungsten_core::terms::SpannedTerm;
use tungsten_core::Type;

use crate::comparator::mangling::comparator_symbol;
use crate::comparator::terms;
use crate::elaborate::CoreDef;
use crate::span::Span;

use super::{comparator_fn_ty, compare_result_named};

/// If `ty` is a cons-list `μα. NonRec + (Elem × α)` — the recursive occurrence
/// appears **only** in the tail (second) position of the `Cons` product, and
/// neither `NonRec` nor `Elem` mentions `α` — return `Elem`. Such lists get the
/// stack-safe tail-recursive spine (P4); other `μ` shapes (e.g. trees, where `α`
/// appears in multiple positions) keep native recursion via `mu_comparator`.
pub(super) fn as_cons_list(ty: &Type) -> Option<Type> {
    let Type::Mu(v, body) = ty else { return None };
    let Type::Sum(nonrec, cons) = body.as_ref() else {
        return None;
    };
    let Type::Product(elem, tail) = cons.as_ref() else {
        return None;
    };
    let Type::TyVar(tv) = tail.as_ref() else {
        return None;
    };
    if tv == v && !mentions_tyvar(nonrec, v) && !mentions_tyvar(elem, v) {
        Some(elem.as_ref().clone())
    } else {
        None
    }
}

/// Whether `var` occurs free-ish as a `TyVar` anywhere in `ty`. Sufficient for the
/// cons-list check: we only need to know if the recursive variable appears outside
/// the tail position (which would make the shape non-list).
fn mentions_tyvar(ty: &Type, var: &str) -> bool {
    match ty {
        Type::TyVar(n) => n == var,
        Type::Product(a, b) | Type::Sum(a, b) | Type::Arrow(a, b) => {
            mentions_tyvar(a, var) || mentions_tyvar(b, var)
        }
        Type::Mu(inner, body) => inner != var && mentions_tyvar(body, var),
        Type::App(_, args) => args.iter().any(|t| mentions_tyvar(t, var)),
        Type::Adt(_, _, variants) => variants.iter().any(|(_, t)| mentions_tyvar(t, var)),
        _ => false,
    }
}

/// Synthesize the wrapper + tail-recursive spine for a cons-list `list_ty` with
/// element type `elem` (ADR 29.6.26f P4). Returns `[wrapper, spine]` and the
/// element type as the only sub-comparator to close over.
pub(super) fn synth_list_comparator(list_ty: &Type, elem: &Type) -> (Vec<CoreDef>, Vec<Type>) {
    let wrapper_sym = comparator_symbol(list_ty);
    let spine_sym = format!("{wrapper_sym}_spine");
    let elem_sym = comparator_symbol(elem);

    let wrapper = CoreDef {
        name: wrapper_sym,
        ty: comparator_fn_ty(list_ty),
        term: SpannedTerm::generated(terms::curry2(
            list_ty.clone(),
            terms::list_wrapper_body(&spine_sym),
        )),
        span: Span::new(0, 0),
    };
    let spine = CoreDef {
        name: spine_sym.clone(),
        ty: spine_fn_ty(list_ty),
        term: SpannedTerm::generated(terms::curry3(
            list_ty.clone(),
            terms::list_spine_body(list_ty, &elem_sym, &spine_sym),
        )),
        span: Span::new(0, 0),
    };
    (vec![wrapper, spine], vec![elem.clone()])
}

/// The spine's curried type `τ → τ → Nat → CompareResult`.
fn spine_fn_ty(operand_ty: &Type) -> Type {
    Type::Arrow(
        Box::new(operand_ty.clone()),
        Box::new(Type::Arrow(
            Box::new(operand_ty.clone()),
            Box::new(Type::Arrow(
                Box::new(Type::Nat),
                Box::new(compare_result_named()),
            )),
        )),
    )
}
