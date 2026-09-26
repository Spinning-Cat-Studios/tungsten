//! Core IR `Term` builders for synthesized structural comparators
//! (ADR 29.6.26f, design doc §T11).
//!
//! The bootstrap cannot register an ADT `TypeDef` programmatically, so the `CompareResult`
//! ADT is *authored* in `.tg` (`src/compiler/driver/ffi/compare/mod.tg`) while the
//! per-type `compare_T` functions are *synthesized* as `CoreDef`s. These helpers
//! build the `Term`s for those functions.
//!
//! Reference shapes below were grounded against `tungsten info def` output, not
//! guessed (CLAUDE.md anti-pattern: "Guessing encodings"). The leaf comparator
//! `compare_Nat` lowers to:
//!
//! ```text
//! λa:Nat. λb:Nat.
//!   if EQ(a, b) then (inl [Unit + CompareDiff] ())
//!               else (inr [Unit + CompareDiff] EMPTY_DIFF)
//! ```
//!
//! where `EMPTY_DIFF` is the (T-independent) `CompareDiff` record
//! `(Nil, ("", ""))` — first-cut failure payload; path/display materialisation
//! is ADR-P5. Named types appear as `TyVar("CompareDiff")` / `TyVar("PathSeg")`
//! in annotations and are resolved at codegen via `adt_types`/`record_types`
//! (CLAUDE.md: "Used by codegen to expand TyVar(RecordName)").

use tungsten_core::{Term, Type};

/// μ-binder name used by the `List<PathSeg>` encoding (matches elaborator output).
const LIST_MU_VAR: &str = "α_List";

/// A named-type reference as it appears in synthesized Core annotations.
fn named(name: &str) -> Type {
    Type::TyVar(name.to_string())
}

/// Encoded sum type of `CompareResult` = `Unit + CompareDiff`.
#[must_use]
pub fn compare_result_sum_ty() -> Type {
    Type::Sum(Box::new(Type::Unit), Box::new(named("CompareDiff")))
}

/// `Equal : CompareResult` — constructor index 0 of a binary sum → `inl`.
#[must_use]
pub fn equal_term() -> Term {
    Term::Inl(compare_result_sum_ty(), Box::new(Term::Unit))
}

/// `NotEqual(diff) : CompareResult` — constructor index 1 → `inr`.
#[must_use]
pub fn not_equal_term(diff: Term) -> Term {
    Term::Inr(compare_result_sum_ty(), Box::new(diff))
}

/// The `List<PathSeg>` (`ComparePath`) encoding: `μα_List. Unit + (PathSeg × α_List)`.
///
/// The μ-body uses the bound variable `α_List` in the tail position (not the
/// full μ-type) — re-expanding it here would diverge.
#[must_use]
pub fn compare_path_mu_ty() -> Type {
    Type::Mu(
        LIST_MU_VAR.to_string(),
        Box::new(compare_path_sum_ty(false)),
    )
}

/// The unfolded sum of `ComparePath`. When `recursive_is_mu` the tail position
/// holds the full μ-type (as in the inner `inl` annotation); otherwise the bound
/// variable `α_List` (as in the `μ` body).
fn compare_path_sum_ty(recursive_is_mu: bool) -> Type {
    let tail = if recursive_is_mu {
        compare_path_mu_ty()
    } else {
        Type::TyVar(LIST_MU_VAR.to_string())
    };
    Type::Sum(
        Box::new(Type::Unit),
        Box::new(Type::Product(Box::new(named("PathSeg")), Box::new(tail))),
    )
}

/// Empty `ComparePath` (`Nil`): `fold [μ…] (inl [Unit + (PathSeg × μ…)] ())`.
#[must_use]
pub fn empty_path_term() -> Term {
    let inner = Term::Inl(compare_path_sum_ty(true), Box::new(Term::Unit));
    Term::Fold(compare_path_mu_ty(), Box::new(inner))
}

/// Empty `CompareDiff` record `(path, (left_display, right_display))` with an
/// empty path and empty displays. First-cut failure payload (ADR-P5 fills it in).
#[must_use]
pub fn empty_diff_term() -> Term {
    Term::Pair(
        Box::new(empty_path_term()),
        Box::new(Term::Pair(
            Box::new(Term::StringLit(String::new())),
            Box::new(Term::StringLit(String::new())),
        )),
    )
}

// ── First-differing-path construction (ADR 29.6.26f §2.3 / P5) ──────────────
//
// The structural first cut emits `Pos(i)` for positional descent and `Tag` for a
// tag/length mismatch. Source-level refinements (record field names `.field`,
// flat list `[index]` / `.len`) need source-type metadata and are deferred;
// `left_display`/`right_display` stay empty pending a Core value→string
// primitive (none exists today).

/// `PathSeg` flat-ADT indices (source-declaration order): `Field`=0, `Pos`=1,
/// `Index`=2, `Tag`=3, `Len`=4.
pub(super) fn path_seg(idx: usize, payload: Term) -> Term {
    Term::AdtConstruct(named("PathSeg"), idx, Box::new(payload))
}

/// `Pos(n)` — a positional descent segment.
#[must_use]
pub fn seg_pos(n: u64) -> Term {
    path_seg(1, Term::NatLit(n))
}

/// `Tag` — a constructor tag (or list length) mismatch segment.
#[must_use]
pub fn seg_tag() -> Term {
    path_seg(3, Term::Unit)
}

// Record `.field` path builders (ADR 29.6.26f §2.3 / AC 8) live in the `record`
// submodule.
mod record;
pub use record::{record_comparator_body, seg_field};

/// `Cons(seg, tail) : List<PathSeg>`.
pub(super) fn cons_path(seg: Term, tail: Term) -> Term {
    let pair = Term::Pair(Box::new(seg), Box::new(tail));
    let inner = Term::Inr(compare_path_sum_ty(true), Box::new(pair));
    Term::Fold(compare_path_mu_ty(), Box::new(inner))
}

/// Singleton path `[seg]`.
#[must_use]
pub fn single_path(seg: Term) -> Term {
    cons_path(seg, empty_path_term())
}

/// `CompareDiff { path, "", "" }` — displays empty (pending a Core show primitive).
fn diff_with_path(path: Term) -> Term {
    Term::Pair(
        Box::new(path),
        Box::new(Term::Pair(
            Box::new(Term::StringLit(String::new())),
            Box::new(Term::StringLit(String::new())),
        )),
    )
}

/// `NotEqual` carrying `path`.
#[must_use]
pub fn not_equal_path(path: Term) -> Term {
    not_equal_term(diff_with_path(path))
}

/// Prepend `seg` to a comparison result's path: `Equal` stays `Equal`; `NotEqual`
/// gets `seg` consed onto its path (displays unchanged).
///
/// `case result of Inl _ => Equal | Inr d => Inr (cons(seg, d.path), d.displays)`.
#[must_use]
pub fn prepend_seg(result: Term, seg: Term) -> Term {
    let d = || Term::Var("d".to_string());
    let new_diff = Term::Pair(
        Box::new(cons_path(seg, Term::Fst(Box::new(d())))),
        Box::new(Term::Snd(Box::new(d()))),
    );
    Term::Case(
        Box::new(result),
        "_e".to_string(),
        Box::new(equal_term()),
        "d".to_string(),
        Box::new(Term::Inr(compare_result_sum_ty(), Box::new(new_diff))),
    )
}

/// Leaf comparator body: `if eq then Equal else NotEqual(empty_diff)`. The leaf
/// is the root of its own comparison (`path = Nil`); enclosing comparators
/// prepend the segment that reaches it.
#[must_use]
pub fn leaf_compare_body(eq: Term) -> Term {
    Term::If(
        Box::new(eq),
        Box::new(equal_term()),
        Box::new(not_equal_term(empty_diff_term())),
    )
}

/// Full leaf comparator term: `λa:τ. λb:τ. <leaf_compare_body(mk_eq(a, b))>`.
///
/// `mk_eq` builds the per-leaf equality predicate over the two bound operands.
#[must_use]
pub fn leaf_comparator_term(operand_ty: Type, mk_eq: impl Fn(Term, Term) -> Term) -> Term {
    let eq = mk_eq(Term::Var("a".to_string()), Term::Var("b".to_string()));
    let body = leaf_compare_body(eq);
    Term::Lambda(
        "a".to_string(),
        operand_ty.clone(),
        Box::new(Term::Lambda("b".to_string(), operand_ty, Box::new(body))),
    )
}

/// Wrap a comparator body in the two operand lambdas `λl:τ. λr:τ. <body>`.
/// `body` may reference the bound operands as `Var("l")` / `Var("r")`.
#[must_use]
pub fn curry2(operand_ty: Type, body: Term) -> Term {
    Term::Lambda(
        "l".to_string(),
        operand_ty.clone(),
        Box::new(Term::Lambda("r".to_string(), operand_ty, Box::new(body))),
    )
}

/// Body of a sum comparator `A + B`: compare tags, then matching payloads;
/// a tag mismatch is `NotEqual` (first-cut empty diff; `.tag` path is ADR-P5).
///
/// ```text
/// case l of
///   Inl la => case r of Inl ra => compare_A(la, ra) | Inr _ => NotEqual
///   Inr lb => case r of Inl _  => NotEqual          | Inr rb => compare_B(lb, rb)
/// ```
#[must_use]
pub fn sum_comparator_body(symbol_a: &str, symbol_b: &str) -> Term {
    let var = |n: &str| Term::Var(n.to_string());
    let tag_mismatch = || not_equal_path(single_path(seg_tag()));
    // Matching tags compare the constructor payload at position `.0`.
    let cmp_a = || prepend_seg(compare_app(symbol_a, var("la"), var("ra")), seg_pos(0));
    let cmp_b = || prepend_seg(compare_app(symbol_b, var("lb"), var("rb")), seg_pos(0));
    let l_is_inl = Term::Case(
        Box::new(var("r")),
        "ra".to_string(),
        Box::new(cmp_a()),
        "rb".to_string(),
        Box::new(tag_mismatch()),
    );
    let l_is_inr = Term::Case(
        Box::new(var("r")),
        "ra".to_string(),
        Box::new(tag_mismatch()),
        "rb".to_string(),
        Box::new(cmp_b()),
    );
    Term::Case(
        Box::new(var("l")),
        "la".to_string(),
        Box::new(l_is_inl),
        "lb".to_string(),
        Box::new(l_is_inr),
    )
}

/// A curried call to a comparator global: `compare_T(left)(right)`.
#[must_use]
pub fn compare_app(symbol: &str, left: Term, right: Term) -> Term {
    Term::App(
        Box::new(Term::App(
            Box::new(Term::Global(symbol.to_string())),
            Box::new(left),
        )),
        Box::new(right),
    )
}

/// Sequence two comparisons, short-circuiting on the first difference:
/// `case first of Equal => second | NotEqual(d) => NotEqual(d)`.
///
/// `CompareResult` is `Inl(())` (Equal) / `Inr(diff)` (NotEqual), so this is a
/// `Case` whose Inl arm runs `second` and whose Inr arm rewraps the bound diff.
#[must_use]
pub fn then_compare(first: Term, second: Term) -> Term {
    Term::Case(
        Box::new(first),
        "_eq".to_string(),
        Box::new(second),
        "d".to_string(),
        Box::new(not_equal_term(Term::Var("d".to_string()))),
    )
}

// List-like μ spine builders (ADR 29.6.26f P4) live in the `list` submodule.
mod list;
pub use list::{curry3, list_spine_body, list_wrapper_body, seg_index, seg_len};

// ── Per-leaf equality predicates ────────────────────────────────────────────

/// `Nat` equality: the `NatEq` primitive.
#[must_use]
pub fn nat_eq(a: Term, b: Term) -> Term {
    Term::NatEq(Box::new(a), Box::new(b))
}

/// `Int` equality: the `IntBin(Eq)` primitive (ADR 14.9.26c).
#[must_use]
pub fn int_eq(a: Term, b: Term) -> Term {
    Term::int_bin(tungsten_core::terms::IntBinOp::Eq, a, b)
}

/// `String` equality: the `StrEq` primitive.
#[must_use]
pub fn str_eq(a: Term, b: Term) -> Term {
    Term::StrEq(Box::new(a), Box::new(b))
}

/// `Bool` equality: `if a then b else !b` (no `BoolEq` primitive exists).
#[must_use]
pub fn bool_eq(a: Term, b: Term) -> Term {
    Term::If(
        Box::new(a),
        Box::new(b.clone()),
        Box::new(Term::BoolNot(Box::new(b))),
    )
}

#[cfg(test)]
mod tests;
