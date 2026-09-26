//! Assembly of synthesized `compare_T` definitions (ADR 29.6.26f, design doc
//! §T11.2/§T11.8).
//!
//! Wraps the [`terms`](super::terms) builders into ordinary [`CoreDef`]s that
//! the compile/eval pipeline injects. Each `compare_T` has the ABI
//! `compare_T(left: T, right: T) -> CompareResult`.
//!
//! Composite comparators reference their field/element comparators by symbol;
//! [`synth_comparator`] returns those referenced sub-types so the caller can
//! transitively close over them (the per-type "cached, deduplicated" design —
//! each type emits once). Currently covers primitive leaves, `Unit`, `Product`
//! (tuples), and `Sum` (`Option` / binary ADTs as they arrive structurally).
//! Named types (records, flat ADTs, `List`) and recursive types + the stack-safe
//! worklist (design doc §T11.3) are later ADR priorities.

use tungsten_core::terms::SpannedTerm;
use tungsten_core::{Term, Type};

use super::context::ComparatorTypes;
use crate::driver::RecordTypes;
use crate::elaborate::CoreDef;

use crate::span::Span;

use super::mangling::comparator_symbol;
use super::terms;

/// The named return type of every comparator, resolved at codegen/eval via
/// `adt_types` (CLAUDE.md: "expand TyVar(RecordName)").
pub(super) fn compare_result_named() -> Type {
    Type::TyVar("CompareResult".to_string())
}

/// The record name a type names (`App`/`TyVar`), if `records` defines it.
fn record_name<'a>(ty: &'a Type, records: &RecordTypes) -> Option<&'a str> {
    match ty {
        Type::App(name, args) if args.is_empty() && records.contains_key(name) => Some(name),
        Type::TyVar(name) if records.contains_key(name) => Some(name),
        _ => None,
    }
}

/// Build the curried function type `τ → τ → CompareResult`.
pub(super) fn comparator_fn_ty(operand_ty: &Type) -> Type {
    Type::Arrow(
        Box::new(operand_ty.clone()),
        Box::new(Type::Arrow(
            Box::new(operand_ty.clone()),
            Box::new(compare_result_named()),
        )),
    )
}

/// Synthesize the comparator `CoreDef` for `ty`, plus the sub-types its body
/// references (for transitive closure). Returns `None` if `ty` is unsupported.
///
/// Returns only the **primary** (top-level) comparator def. Some types synthesize
/// auxiliary defs too (e.g. a list's tail-recursive spine, ADR-P4) — use
/// [`synth_comparator_defs`] to get the full set. Retained for the many unit tests
/// that assert on the primary def's shape.
#[must_use]
pub fn synth_comparator(ty: &Type, types: &ComparatorTypes) -> Option<(CoreDef, Vec<Type>)> {
    let (mut defs, subtypes) = synth_comparator_defs(ty, types)?;
    Some((defs.remove(0), subtypes))
}

/// Synthesize **all** comparator `CoreDef`s for `ty` (primary first), plus the
/// sub-types they reference. Most types emit one def; a list-like `μ` emits two —
/// the wrapper (`compare_List_T`) and its stack-safe tail-recursive spine
/// (`compare_List_T_spine`, ADR 29.6.26f P4).
#[must_use]
pub fn synth_comparator_defs(
    ty: &Type,
    types: &ComparatorTypes,
) -> Option<(Vec<CoreDef>, Vec<Type>)> {
    // List-like `μα. NonRec + (Elem × α)` → stack-safe tail-recursive spine (P4).
    if is_supported(ty, types) {
        if let Some(elem) = list::as_cons_list(ty) {
            return Some(list::synth_list_comparator(ty, &elem));
        }
    }
    let (body, subtypes) = comparator_body(ty, types)?;
    let def = CoreDef {
        name: comparator_symbol(ty),
        ty: comparator_fn_ty(ty),
        term: SpannedTerm::generated(body),
        span: Span::new(0, 0),
    };
    Some((vec![def], subtypes))
}

/// Build a comparator body and the list of sub-types it references.
fn comparator_body(ty: &Type, types: &ComparatorTypes) -> Option<(Term, Vec<Type>)> {
    let ok = |t: &Type| is_supported(t, types);
    match ty {
        Type::Nat => Some((
            terms::leaf_comparator_term(Type::Nat, terms::nat_eq),
            vec![],
        )),
        Type::Int => Some((
            terms::leaf_comparator_term(Type::Int, terms::int_eq),
            vec![],
        )),
        Type::Bool => Some((
            terms::leaf_comparator_term(Type::Bool, terms::bool_eq),
            vec![],
        )),
        Type::String => Some((
            terms::leaf_comparator_term(Type::String, terms::str_eq),
            vec![],
        )),
        // Unit has a single inhabitant — always equal.
        Type::Unit => Some((terms::curry2(Type::Unit, terms::equal_term()), vec![])),
        Type::Product(a, b) if ok(a) && ok(b) => Some(product_comparator(ty, a, b)),
        Type::Sum(a, b) if ok(a) && ok(b) => Some(sum_comparator(ty, a, b)),
        Type::Adt(_, _, variants) if ok(ty) => Some(adt_comparator(ty, variants)),
        Type::Mu(_, _) if ok(ty) => mu_comparator(ty, types),
        // A generic ADT instantiation (`List<TypeParam>`) resolves through
        // `adt_types`; the def keeps the `App` spelling and its body delegates
        // to the expansion's comparator (ADR 1.8.26c).
        Type::App(name, args) if !args.is_empty() && ok(ty) => {
            instantiation_comparator(ty, name, args, types)
        }
        // Named record types (`App` or `TyVar`) resolve to a right-nested
        // Product over their fields; the comparator delegates to its comparator.
        _ => {
            let records = types.records();
            let name = record_name(ty, records)?;
            if ok(ty) {
                Some(record_comparator(ty, &records[name]))
            } else {
                None
            }
        }
    }
}

/// Comparator for a record `App("Name", [])`: compare each field in declaration
/// order, emitting source-level `.field` path segments (ADR 29.6.26f §2.3 / AC 8).
/// A record value *is* the right-nested product over its fields at runtime, so the
/// body projects into it field-by-field. Pulls in each field type's comparator.
fn record_comparator(operand: &Type, fields: &[(String, Type)]) -> (Term, Vec<Type>) {
    let name_sym: Vec<(String, String)> = fields
        .iter()
        .map(|(n, t)| (n.clone(), comparator_symbol(&t.strip_tyvar_at_prefix())))
        .collect();
    let subtypes: Vec<Type> = fields
        .iter()
        .map(|(_, t)| t.strip_tyvar_at_prefix())
        .collect();
    let body = terms::record_comparator_body(&name_sym);
    (terms::curry2(operand.clone(), body), subtypes)
}

/// Comparator for a flat ADT: match the left tag, then match the right tag;
/// matching tags compare payloads, mismatched tags ⇒ `NotEqual`. References each
/// variant's payload comparator.
///
/// ```text
/// λl. λr. match l { iᵢ(p) => match r { iᵢ(q) => compare_Tᵢ(p, q) | _ => NotEqual } }
/// ```
fn adt_comparator(operand: &Type, variants: &[(String, Type)]) -> (Term, Vec<Type>) {
    let outer_arms = variants
        .iter()
        .enumerate()
        .map(|(i, (_, payload_ty))| {
            let symbol_i = comparator_symbol(payload_ty);
            let inner_arms = (0..variants.len())
                .map(|j| {
                    let body = if j == i {
                        // Matching tags: compare the payload at position `.0`.
                        terms::prepend_seg(
                            terms::compare_app(
                                &symbol_i,
                                Term::Var("p".to_string()),
                                Term::Var("q".to_string()),
                            ),
                            terms::seg_pos(0),
                        )
                    } else {
                        terms::not_equal_path(terms::single_path(terms::seg_tag()))
                    };
                    (j, "q".to_string(), Box::new(body))
                })
                .collect();
            let inner = Term::AdtMatch(Box::new(Term::Var("r".to_string())), inner_arms);
            (i, "p".to_string(), Box::new(inner))
        })
        .collect();
    let body = Term::AdtMatch(Box::new(Term::Var("l".to_string())), outer_arms);
    let subtypes = variants.iter().map(|(_, t)| t.clone()).collect();
    (terms::curry2(operand.clone(), body), subtypes)
}

/// Comparator for a recursive type: unfold both operands **once** and compare
/// the resolved body.
///
/// # Once, however many binders (ADR 1.8.26b D2)
///
/// A mutually recursive cluster encodes as one nested μ-binder per SCC member
/// — `Alpha = μα_Alpha. μα_Beta. μα_Gamma. …` — while a *value* carries exactly
/// one `Fold`, applied by its constructor. Recursing binder-by-binder therefore
/// emitted three `Unfold`s against one `Fold`, and the second one hit a bare
/// sum injection: `Unfold` of a non-`Fold`, which stepped to Stuck, so the
/// enclosing assertion never ran and the test reported `ok`. The whole chain is
/// peeled here and exactly one `Unfold` is emitted.
///
/// # Resolving the other members
///
/// Each binder stands for one cluster member. The outermost is this very type,
/// so it substitutes to `operand`. The rest are *other* members whose bodies
/// the encoding does not carry (see [`ComparatorTypes`]), so they resolve
/// through the provenance map. `None` when a binder cannot be resolved — the
/// gate then reports it rather than emitting a comparator that would compare
/// the wrong member's shape.
fn mu_comparator(operand: &Type, types: &ComparatorTypes) -> Option<(Term, Vec<Type>)> {
    let (binders, body) = peel_mu_chain(operand);
    let mut resolved = body.clone();
    for (depth, binder) in binders.iter().enumerate() {
        let member = if depth == 0 {
            operand.clone()
        } else {
            types.mu_member(binder)?.clone()
        };
        resolved = resolved.substitute(binder, &member);
    }
    let call = terms::compare_app(
        &comparator_symbol(&resolved),
        Term::Unfold(operand.clone(), Box::new(Term::Var("l".to_string()))),
        Term::Unfold(operand.clone(), Box::new(Term::Var("r".to_string()))),
    );
    Some((terms::curry2(operand.clone(), call), vec![resolved]))
}

/// Comparator for a **generic ADT instantiation** `Name<A, …>` (ADR 1.8.26c):
/// expand the ADT and delegate to the expansion's comparator.
///
/// # Why the def keeps the `App` spelling
///
/// Call sites mangle sub-types as they stand, and `mangle` already emits
/// `App<name>_<args>E` for a non-empty `App`. Expanding at the head of
/// `synth_comparator_defs` instead would name the def after the *expanded*
/// μ-type while the caller emitted the `App` spelling — a dangling symbol,
/// which is ADR 1.8.26b D2's failure shape returning. So this mirrors
/// [`mu_comparator`]: keep `operand` as the def's name, expand inside the body,
/// and return the expansion as a sub-type.
///
/// # Why no `Unfold`
///
/// Unlike `mu_comparator` this emits a plain call. `List<T>` and its expansion
/// `μα_List. …` are the same type, so the value arrives already folded and the
/// **expansion's own** comparator is the one that unfolds it. Unfolding here as
/// well would meet a single `Fold` with two `Unfold`s — the D2 defect.
///
/// # Why the expansion must be *returned*
///
/// `as_cons_list`'s probe runs at the head of [`synth_comparator_defs`], i.e.
/// before `comparator_body`, so the `App` itself never matches it. Returning the
/// expansion as a sub-type is what re-enters the walk with the μ, where the
/// probe fires and routes into the stack-safe tail-recursive spine (ADR
/// 29.6.26f P4). Drop the sub-type and a fixture still passes while a long
/// `List<Stmt>` gets a stack-recursive comparator and overflows on real input.
fn instantiation_comparator(
    operand: &Type,
    name: &str,
    args: &[Type],
    types: &ComparatorTypes,
) -> Option<(Term, Vec<Type>)> {
    let expanded = types.expand_adt(name, args)?;
    let call = terms::compare_app(
        &comparator_symbol(&expanded),
        Term::Var("l".to_string()),
        Term::Var("r".to_string()),
    );
    Some((terms::curry2(operand.clone(), call), vec![expanded]))
}

/// Split `μa. μb. … μz. body` into its binder names (outermost first) and the
/// body underneath them all.
fn peel_mu_chain(ty: &Type) -> (Vec<&str>, &Type) {
    let mut binders = Vec::new();
    let mut current = ty;
    while let Type::Mu(var, body) = current {
        binders.push(var.as_str());
        current = body;
    }
    (binders, current)
}

/// Comparator for `A + B`: compare tags, then matching payloads (first tag
/// mismatch ⇒ `NotEqual`). References `compare_A` / `compare_B`.
fn sum_comparator(operand: &Type, a: &Type, b: &Type) -> (Term, Vec<Type>) {
    let body = terms::sum_comparator_body(&comparator_symbol(a), &comparator_symbol(b));
    (
        terms::curry2(operand.clone(), body),
        vec![a.clone(), b.clone()],
    )
}

/// Comparator for a **tuple** `(A × B)`: compare the first projections, and on
/// `Equal` compare the second; the first difference propagates. Emits source-level
/// `[index]` path segments (ADR 29.6.26f §2.3 / AC 8). Records reach their fields via
/// `record_comparator` (`.field`), so this path is now tuple-only. (For a flat N-tuple
/// `N > 2` the right-nested encoding yields nested `[1][…]` indices rather than a flat
/// `[i]` — an encoding-structural limitation; binary tuples are flat.)
///
/// `λl. λr. case compare_A(fst l, fst r) of
///            Equal      => compare_B(snd l, snd r)
///            NotEqual d => NotEqual d`
fn product_comparator(operand: &Type, a: &Type, b: &Type) -> (Term, Vec<Type>) {
    let l = || Term::Var("l".to_string());
    let r = || Term::Var("r".to_string());
    let cmp_first = terms::prepend_seg(
        terms::compare_app(
            &comparator_symbol(a),
            Term::Fst(Box::new(l())),
            Term::Fst(Box::new(r())),
        ),
        terms::seg_index(Term::NatLit(0)),
    );
    let cmp_second = terms::prepend_seg(
        terms::compare_app(
            &comparator_symbol(b),
            Term::Snd(Box::new(l())),
            Term::Snd(Box::new(r())),
        ),
        terms::seg_index(Term::NatLit(1)),
    );
    let body = terms::then_compare(cmp_first, cmp_second);
    (
        terms::curry2(operand.clone(), body),
        vec![a.clone(), b.clone()],
    )
}

mod list;
pub mod support;

pub use support::{check_comparable, is_supported, Noncomparable, INSTANTIATION_DEPTH_CAP};

#[cfg(test)]
mod tests;

#[cfg(test)]
// Tests: synth/instantiation_tests.rs
#[path = "synth/instantiation_tests.rs"]
mod instantiation_tests;
