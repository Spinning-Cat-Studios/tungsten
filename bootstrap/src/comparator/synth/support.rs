//! Which types comparator synthesis can handle at all.
//!
//! **One walk, two views.** [`check_comparable`] decides, and reports the path
//! to the first offender; [`is_supported`] is that succeeding. A type one
//! accepted while the other rejected would make `doctor check comparable` lie
//! about what a run will do, so the two are the same function rather than two
//! functions plus a test that they agree.
//!
//! They *were* two mirrored walkers (ADR 1.8.26b, doubled to four functions by
//! 1.8.26c's instantiation arm), because building `format!("{path}.0")` on
//! every node was too expensive for the synthesis path — `comparator_body`
//! calls `is_supported` twice per composite node. [`ComponentPath`] removes
//! that reason: the path is a linked stack frame, rendered only on failure, so
//! the successful walk allocates nothing for it.
//!
//! Split from `synth.rs` for the file-size limit, along the seam that matters:
//! this file decides *whether*, `synth.rs` decides *how*.

use std::collections::HashSet;

use tungsten_core::Type;

use crate::comparator::context::ComparatorTypes;
use crate::comparator::mangling::comparator_symbol;

/// How many nested **instantiation expansions** one walk may perform before it
/// reports the type unsettled.
///
/// The structural cycle key below stops a walk that revisits the *same*
/// instantiation, which covers every recursion the compiler's own AST contains
/// (`List<Expr>` inside `Expr`). It does not stop **polymorphic recursion**,
/// where each round reaches a strictly larger argument and so never repeats:
/// `Nest<T> = Nil | Cons(T, Nest<(T, T)>)` expands through `Nest<(T,T)>`,
/// `Nest<((T,T),(T,T))>`, … forever. Only a depth bound terminates that.
///
/// The bound must be small, not merely finite: the argument doubles each round,
/// so the type at depth *d* is O(2^d) nodes and a generous cap would trade a
/// hang for a hang. Sixteen is ~3 orders of magnitude above any nesting real
/// code writes (`List<List<Nat>>` is 2; the compiler's AST reaches 1) while
/// bounding the pathological walk at ~10⁵ nodes — milliseconds.
///
/// Exhaustion is reported as [`Noncomparable::Unsettled`], **not** as an opaque
/// leaf: "we stopped looking" and "noncomparable by policy" call for different
/// fixes, and conflating them would tell a reader to normalize away a field
/// that is in fact fine.
pub const INSTANTIATION_DEPTH_CAP: usize = 16;

/// Why a type is not comparable.
///
/// The two variants exist because their fixes differ, and because `classify`
/// maps them to different `ComparatorFailureKind`s. Before ADR 1.8.26c the
/// diagnostic returned a bare `String` and every failure became an opaque leaf,
/// so a bound exhaustion would have been reported as a *policy* decision about
/// the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Noncomparable {
    /// Noncomparable by policy (ADR 29.6.26f §2.2): `path` locates the first
    /// offending component, e.g. `$.env: EvalEnv is opaque`.
    Opaque(String),
    /// The walk hit its instantiation-expansion bound with work still queued,
    /// so no verdict about the type itself was reached.
    Unsettled(usize),
}

impl std::fmt::Display for Noncomparable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Noncomparable::Opaque(path) => write!(f, "{path}"),
            Noncomparable::Unsettled(bound) => write!(
                f,
                "expansion did not settle within {bound} nested instantiations"
            ),
        }
    }
}

/// State threaded through one comparability walk.
///
/// Bundled rather than passed loose because the two recursive walkers were
/// already at four parameters and this adds two more (ADR 1.8.26c §3) — and
/// because the two sets must stay paired: dropping either one silently changes
/// which types terminate.
#[derive(Clone, Default)]
struct WalkState {
    /// Names treated as comparable to break recursion: μ-bound variables (the
    /// enclosing recursive type) and records currently being checked.
    assumed: HashSet<String>,
    /// Instantiations already being expanded on this path, keyed
    /// **structurally** (by comparator symbol, which mangles the arguments).
    ///
    /// A name-keyed set would accept `List<TypeExpr>` on the strength of having
    /// seen `List<TypeParam>`, and the walk would then report a type comparable
    /// while synthesis emitted a call to a comparator the closure never defines
    /// — the incomplete-closure class ADR 1.8.26b spent itself eliminating.
    expanding: HashSet<String>,
    /// How many expansions deep this path is, against [`INSTANTIATION_DEPTH_CAP`].
    depth: usize,
}

impl WalkState {
    /// This state plus `name` assumed comparable.
    fn assuming(&self, name: &str) -> WalkState {
        let mut next = self.clone();
        next.assumed.insert(name.to_string());
        next
    }

    /// This state one expansion deeper, or `None` when that would exceed
    /// [`INSTANTIATION_DEPTH_CAP`].
    fn descend(&self, key: String) -> Option<WalkState> {
        if self.depth >= INSTANTIATION_DEPTH_CAP {
            return None;
        }
        let mut next = self.clone();
        next.depth += 1;
        next.expanding.insert(key);
        Some(next)
    }
}

/// One step of the path to the component being checked.
#[derive(Clone, Copy)]
enum Segment<'a> {
    /// A positional component: a product side, or an ADT variant.
    Index(usize),
    /// A sum side.
    Inl,
    Inr,
    /// A named record field, borrowed from the record map.
    Field(&'a str),
}

/// The path to the component currently being checked, built on the **stack**
/// and rendered only when a walk fails.
///
/// This is what lets the yes/no predicate and the diagnostic be one function
/// (ADR 1.8.26c retrospective). They were two mirrored walkers, kept in step by
/// a single agreement test, because the diagnostic's `format!("{path}.0")` per
/// node was too expensive to pay on the synthesis path — `comparator_body`
/// calls `is_supported` twice per composite node. A linked frame costs one
/// stack slot and no allocation, so the reason to keep them apart is gone.
struct ComponentPath<'a> {
    parent: Option<&'a ComponentPath<'a>>,
    segment: Segment<'a>,
}

impl ComponentPath<'_> {
    /// Render root-first, e.g. `$.3.inr.name`. Called only on failure.
    fn render(path: Option<&ComponentPath>) -> String {
        let mut segments = Vec::new();
        let mut cursor = path;
        while let Some(frame) = cursor {
            segments.push(frame.segment);
            cursor = frame.parent;
        }
        let mut out = String::from("$");
        for segment in segments.iter().rev() {
            match segment {
                Segment::Index(i) => out.push_str(&format!(".{i}")),
                Segment::Inl => out.push_str(".inl"),
                Segment::Inr => out.push_str(".inr"),
                Segment::Field(name) => out.push_str(&format!(".{name}")),
            }
        }
        out
    }
}

/// Extend `parent` by one segment.
fn step<'a>(parent: Option<&'a ComponentPath<'a>>, segment: Segment<'a>) -> ComponentPath<'a> {
    ComponentPath { parent, segment }
}

/// Whether this build can synthesize a comparator for `ty`. A type is supported
/// iff every reachable component is supported (a crude precursor to the
/// `Comparable<T>` intrinsic of ADR-P3). `types` resolves named record types
/// (`App("Name", [])`), μ-cluster members, and generic ADT instantiations
/// (`App("List", [T])`).
///
/// Defined **as** [`check_comparable`] succeeding, so the two cannot disagree
/// by construction. Returning a bool it cannot distinguish "noncomparable by
/// policy" from "the expansion did not settle" — it conservatively answers
/// `false` for both; callers that must tell them apart use `check_comparable`.
#[must_use]
pub fn is_supported(ty: &Type, types: &ComparatorTypes) -> bool {
    check_comparable(ty, types).is_ok()
}

/// `Ok(())` if `ty` is comparable, or the reason if not — either the path to
/// the first incomparable component (e.g. `$.env: EvalEnv is opaque`) or an
/// unsettled expansion.
pub fn check_comparable(ty: &Type, types: &ComparatorTypes) -> Result<(), Noncomparable> {
    check_comparable_rec(ty, &WalkState::default(), types, None)
}

/// Internal recursive version that tracks the path.
fn check_comparable_rec(
    ty: &Type,
    state: &WalkState,
    types: &ComparatorTypes,
    path: Option<&ComponentPath>,
) -> Result<(), Noncomparable> {
    match ty {
        Type::Nat | Type::Int | Type::Bool | Type::String | Type::Unit => Ok(()),
        Type::Product(a, b) => {
            check_comparable_rec(a, state, types, Some(&step(path, Segment::Index(0))))?;
            check_comparable_rec(b, state, types, Some(&step(path, Segment::Index(1))))
        }
        Type::Sum(a, b) => {
            check_comparable_rec(a, state, types, Some(&step(path, Segment::Inl)))?;
            check_comparable_rec(b, state, types, Some(&step(path, Segment::Inr)))
        }
        Type::Adt(_, _, variants) => {
            for (i, (_, field_ty)) in variants.iter().enumerate() {
                check_comparable_rec(field_ty, state, types, Some(&step(path, Segment::Index(i))))?;
            }
            Ok(())
        }
        Type::Mu(v, body) => check_comparable_rec(body, &state.assuming(v), types, path),
        Type::App(_, args) if !args.is_empty() => check_instantiation(ty, state, types, path),
        Type::App(name, _) => check_record_comparable(name, state, types, path),
        Type::TyVar(n) => check_record_comparable(n, state, types, path),
        _ => Err(Noncomparable::Opaque(format!(
            "{}: {:?} is opaque",
            ComponentPath::render(path),
            ty
        ))),
    }
}

/// Check a generic ADT instantiation by expanding it and checking the
/// expansion (ADR 1.8.26c).
///
/// An instantiation already on this path is **accepted**, exactly as a
/// recursive record name is: the comparator for it is being built, so the
/// reference resolves. A project that defines no such ADT is an opaque leaf
/// naming the application, which is what a generic *alias* or *record* is (§2
/// Non-Goals) — refused rather than silently accepted.
fn check_instantiation(
    ty: &Type,
    state: &WalkState,
    types: &ComparatorTypes,
    path: Option<&ComponentPath>,
) -> Result<(), Noncomparable> {
    let opaque = || {
        Noncomparable::Opaque(format!(
            "{}: {:?} is opaque",
            ComponentPath::render(path),
            ty
        ))
    };
    // Reachable only from the `App` arm above; anything else has no
    // instantiation to expand and is an opaque leaf by the same rule.
    let Type::App(name, args) = ty else {
        return Err(opaque());
    };
    let key = comparator_symbol(ty);
    if state.expanding.contains(&key) {
        return Ok(());
    }
    let Some(expanded) = types.expand_adt(name, args) else {
        return Err(opaque());
    };
    let Some(deeper) = state.descend(key) else {
        return Err(Noncomparable::Unsettled(INSTANTIATION_DEPTH_CAP));
    };
    check_comparable_rec(&expanded, &deeper, types, path)
}

/// Check if a named record type is comparable, tracking field paths.
fn check_record_comparable(
    name: &str,
    state: &WalkState,
    types: &ComparatorTypes,
    path: Option<&ComponentPath>,
) -> Result<(), Noncomparable> {
    if state.assumed.contains(name) {
        Ok(())
    } else if let Some(fields) = types.records().get(name) {
        let inner = state.assuming(name);
        for (field_name, field_ty) in fields {
            check_comparable_rec(
                &field_ty.strip_tyvar_at_prefix(),
                &inner,
                types,
                Some(&step(path, Segment::Field(field_name))),
            )?;
        }
        Ok(())
    } else {
        Err(Noncomparable::Opaque(format!(
            "{}: {} is not defined",
            ComponentPath::render(path),
            name
        )))
    }
}

#[cfg(test)]
// Tests: support_tests.rs
#[path = "support_tests.rs"]
mod tests;
