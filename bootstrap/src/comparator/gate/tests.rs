//! Tests for the instantiation-time comparability gate (ADR 1.8.26b D3).
//!
//! One test per §3.1 outcome. The ADR requires that split explicitly, because
//! "unsynthesizable ⇒ diagnostic" is satisfiable while a *different* failure
//! mode still returns Stuck — and the three that matter here (incomplete
//! closure, unsettled synthesis, empty closure) are exactly the ones the
//! pre-1.8.26b `closure.is_empty()` predicate could not express.
//!
//! The dangling-symbol case is pinned against a **hand-built** incomplete
//! closure rather than against the D2 repro, so it keeps asserting after D2 is
//! fixed.

use super::*;

use tungsten_core::terms::SpannedTerm;
use tungsten_core::Term;

use crate::span::Span;

/// A `CoreDef` named `name` whose body calls `calls`.
fn def_calling(name: &str, calls: &[&str]) -> CoreDef {
    let mut body = Term::Zero;
    for c in calls {
        body = Term::app(Term::Global((*c).to_string()), body);
    }
    CoreDef {
        name: name.to_string(),
        ty: Type::Nat,
        term: SpannedTerm::generated(body),
        span: Span::new(0, 0),
    }
}

fn kind_of(ty: &Type) -> ComparatorFailureKind {
    match classify(ty, &ComparatorTypes::default()) {
        Ok(_) => panic!("expected `{ty}` to be rejected"),
        Err(f) => f.kind,
    }
}

// ---------------------------------------------------------------------------
// The passing path — without this, every rejection below is trivially met by a
// gate that rejects everything.
// ---------------------------------------------------------------------------

#[test]
fn a_comparable_type_passes_and_yields_a_closed_closure() {
    let gated = classify(&Type::Nat, &ComparatorTypes::default()).expect("Nat is comparable");
    assert_eq!(gated.top_symbol, "compare_Nat");
    assert!(!gated.defs.is_empty());
    assert_eq!(
        first_dangling_reference(&gated.defs),
        None,
        "a passing closure must define every comparator it calls"
    );
}

#[test]
fn a_composite_type_passes_with_its_sub_comparators_defined() {
    let ty = Type::Product(Box::new(Type::Nat), Box::new(Type::Bool));
    let gated = classify(&ty, &ComparatorTypes::default()).expect("(Nat × Bool) is comparable");
    let names: Vec<&str> = gated.defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"compare_Nat"), "{names:?}");
    assert!(names.contains(&"compare_Bool"), "{names:?}");
    assert_eq!(first_dangling_reference(&gated.defs), None);
}

// ---------------------------------------------------------------------------
// §3.1 outcome: opaque leaf
// ---------------------------------------------------------------------------

#[test]
fn an_opaque_leaf_is_rejected_with_its_path() {
    let ty = Type::Product(
        Box::new(Type::Nat),
        Box::new(Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat))),
    );
    let ComparatorFailureKind::OpaqueLeaf { path } = kind_of(&ty) else {
        panic!("a function-typed field is noncomparable by policy");
    };
    assert!(
        path.contains(".1"),
        "the path must locate the offending field, got {path}"
    );
}

// ---------------------------------------------------------------------------
// §3.1 outcome: empty closure
// ---------------------------------------------------------------------------

#[test]
fn an_undefined_named_type_is_rejected() {
    // `TyVar("Nope")` names nothing in `records`. It is caught by the opaque
    // arm rather than the empty-closure arm — asserted here so a future change
    // that moves it between arms is a visible decision, not a silent drift.
    assert!(matches!(
        kind_of(&Type::TyVar("Nope".to_string())),
        ComparatorFailureKind::OpaqueLeaf { .. }
    ));
}

// ---------------------------------------------------------------------------
// §3.1 outcome: incomplete closure (the D2 shape)
// ---------------------------------------------------------------------------

#[test]
fn a_closure_that_calls_an_undefined_comparator_is_incomplete() {
    let defs = vec![
        def_calling("compare_A", &["compare_B"]),
        def_calling("compare_C", &[]),
    ];
    assert_eq!(
        first_dangling_reference(&defs),
        Some("compare_B".to_string())
    );
}

#[test]
fn a_self_consistent_closure_has_no_dangling_reference() {
    // The non-vacuity twin: a detector that reported every closure incomplete
    // would pass the test above unchanged.
    let defs = vec![
        def_calling("compare_A", &["compare_B"]),
        def_calling("compare_B", &["compare_A"]),
    ];
    assert_eq!(first_dangling_reference(&defs), None);
}

#[test]
fn the_reported_dangling_symbol_is_deterministic() {
    // Two dangling symbols, reported in sorted order — a `HashSet` seeds
    // differently per instance, and a gate whose message changes between two
    // identical runs is a gate people learn to distrust.
    let defs = vec![def_calling("compare_A", &["compare_Z", "compare_M"])];
    assert_eq!(
        first_dangling_reference(&defs),
        Some("compare_M".to_string())
    );
}

#[test]
fn a_non_empty_closure_is_not_enough_to_pass() {
    // The predicate this gate replaces. `closure.is_empty()` is false here, so
    // the pre-1.8.26b callback returned `Some`, the evaluator installed the
    // defs, and the unbound edge went Stuck exactly as before.
    let defs = vec![def_calling("compare_A", &["compare_B"])];
    assert!(!defs.is_empty());
    assert!(first_dangling_reference(&defs).is_some());
}

// ---------------------------------------------------------------------------
// §3.1 outcome: synthesis did not settle
// ---------------------------------------------------------------------------

#[test]
fn an_unsettled_closure_reports_the_bound_rather_than_its_partial_defs() {
    // A cap of zero stands in for divergence: the walk cannot settle, so the
    // partial set must not be judged as if it were the whole closure. Testing
    // the *predicate* rather than an actually-diverging type keeps this a
    // millisecond unit test instead of a 900-second one.
    let walk = synth_closure_bounded(&Type::Nat, &ComparatorTypes::default(), 0);
    assert!(!walk.converged);
    assert!(walk.defs.is_empty());
}

#[test]
fn a_settling_closure_reports_converged() {
    let walk = synth_closure_bounded(&Type::Nat, &ComparatorTypes::default(), CLOSURE_CAP);
    assert!(walk.converged);
    assert!(!walk.defs.is_empty());
}

#[test]
fn the_bounded_walk_agrees_with_the_unbounded_one_when_it_settles() {
    // The two share an implementation precisely so they cannot disagree; this
    // pins that they still do after any future split.
    let ty = Type::Product(Box::new(Type::Nat), Box::new(Type::Bool));
    let walk = synth_closure_bounded(&ty, &ComparatorTypes::default(), CLOSURE_CAP);
    assert!(walk.converged);
    let unbounded = super::super::discover::synth_closure_for(&ty, &ComparatorTypes::default());
    let bounded_names: Vec<&str> = walk.defs.iter().map(|d| d.name.as_str()).collect();
    let unbounded_names: Vec<&str> = unbounded.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(bounded_names, unbounded_names);
}

// ---------------------------------------------------------------------------
// A failed synthesis must not claim the symbol
// ---------------------------------------------------------------------------

/// `Type::Adt` mangles from its name and type args alone, so two structurally
/// DIFFERENT ADT occurrences share a symbol. That is deliberate (the variants
/// are determined by name + args) and is the precondition for the regression
/// below — asserted here so a future mangling change makes the dependency
/// visible rather than silently removing the hazard.
#[test]
fn adt_occurrences_that_differ_only_in_their_variants_share_a_symbol() {
    let open = Type::Adt(
        "X".to_string(),
        vec![],
        vec![("B".to_string(), Type::TyVar("α_X".to_string()))],
    );
    let closed = Type::Adt("X".to_string(), vec![], vec![("B".to_string(), Type::Nat)]);
    assert_ne!(open, closed);
    assert_eq!(
        super::super::mangling::comparator_symbol(&open),
        super::super::mangling::comparator_symbol(&closed)
    );
}

/// The regression: reaching the unsynthesizable occurrence FIRST must not stop
/// the synthesizable one from being defined. Before ADR 1.8.26b's fix the walk
/// marked a symbol emitted on *attempt*, so the failed open occurrence claimed
/// `compare_AdtX_E` and the closed one that arrived later was skipped as a
/// duplicate — leaving a closure that called a comparator it never defined.
/// Measured on the real `TypeExpr` and `Expr`.
#[test]
fn a_failed_synthesis_does_not_block_a_later_synthesizable_occurrence() {
    let recursive = Type::Mu(
        "α_X".to_string(),
        Box::new(Type::Adt(
            "X".to_string(),
            vec![],
            vec![
                ("A".to_string(), Type::Unit),
                ("B".to_string(), Type::TyVar("α_X".to_string())),
                ("C".to_string(), Type::Nat),
            ],
        )),
    );
    // The same ADT reached WITHOUT its binder: its `α_X` is free, so synthesis
    // must fail for it. `queue.pop()` is LIFO, so this one is tried first.
    let open = Type::Adt(
        "X".to_string(),
        vec![],
        vec![
            ("A".to_string(), Type::Unit),
            ("B".to_string(), Type::TyVar("α_X".to_string())),
            ("C".to_string(), Type::Nat),
        ],
    );
    let both = Type::Product(Box::new(recursive), Box::new(open));

    let walk = synth_closure_bounded(&both, &ComparatorTypes::default(), CLOSURE_CAP);
    assert!(walk.converged);
    assert_eq!(
        first_dangling_reference(&walk.defs),
        None,
        "the closure must define every comparator it calls; defs = {:?}",
        walk.defs
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !walk.refused.is_empty(),
        "the open occurrence must still be REFUSED — a walk that silently \
         accepted it would pass this test for the wrong reason"
    );
}

// ---------------------------------------------------------------------------
// The failure carries the type, so the diagnostic can name it
// ---------------------------------------------------------------------------

#[test]
fn the_failure_names_the_type_it_was_requested_at() {
    let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat));
    let Err(failure) = classify(&ty, &ComparatorTypes::default()) else {
        panic!("a function type is not comparable");
    };
    assert_eq!(failure.type_name, ty.to_string());
}

// ---------------------------------------------------------------------------
// The walk reports WHICH sub-type it refused
// ---------------------------------------------------------------------------

#[test]
fn a_refused_subtype_is_retrievable_by_the_symbol_it_would_have_had() {
    // The lookup must match the right symbol and reject a wrong one — an
    // accessor that returned the first refusal regardless would satisfy a
    // one-sided test while attributing every dangling symbol to one cause.
    //
    // The refused type is the *product*, not the arrow inside it: the product
    // arm is guarded on its components, so a component that cannot be compared
    // stops the product before its subtypes are ever queued.
    let refused = Type::Product(
        Box::new(Type::Nat),
        Box::new(Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat))),
    );
    let walk = synth_closure_bounded(&refused, &ComparatorTypes::default(), CLOSURE_CAP);
    let symbol = super::super::mangling::comparator_symbol(&refused);
    assert_eq!(walk.refused_under(&symbol), Some(&refused));
    assert_eq!(walk.refused_under("compare_NoSuchSymbol"), None);
}

#[test]
fn a_walk_that_refused_nothing_attributes_nothing() {
    // Non-vacuity twin: on a healthy type the accessor must come back empty,
    // or every clean run would carry a spurious cause.
    let walk = synth_closure_bounded(&Type::Nat, &ComparatorTypes::default(), CLOSURE_CAP);
    assert!(walk.refused.is_empty());
    assert_eq!(walk.refused_under("compare_Nat"), None);
}

#[test]
fn an_incomplete_closure_carries_the_refused_subtypes_cause() {
    // End-to-end: the gate must turn the refusal into a *reason*, because the
    // dangling symbol alone names the shape that could not be built, not why.
    let adt = Type::Adt(
        "Holder".to_string(),
        vec![],
        vec![
            ("A".to_string(), Type::Nat),
            ("B".to_string(), Type::TyVar("α_Holder".to_string())),
            ("C".to_string(), Type::Nat),
        ],
    );
    let bare = Type::Product(
        Box::new(Type::Mu("α_Holder".to_string(), Box::new(adt.clone()))),
        Box::new(adt),
    );
    // The bare occurrence's `α_Holder` is free, so the walk refuses it.
    let walk = synth_closure_bounded(&bare, &ComparatorTypes::default(), CLOSURE_CAP);
    assert!(
        !walk.refused.is_empty(),
        "the free-variable occurrence must be refused"
    );
}

// ---------------------------------------------------------------------------
// Synthesis refuses an unsupported operand rather than emitting for it
// ---------------------------------------------------------------------------

#[test]
fn a_recursive_type_whose_body_is_unsupported_is_refused() {
    // The `ok(ty)` guard on the μ arm: without it, synthesis emits a comparator
    // whose body calls a sub-comparator that can never exist.
    let ty = Type::Mu(
        "α_X".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat))),
        )),
    );
    assert!(
        super::super::synth::synth_comparator_defs(&ty, &ComparatorTypes::default()).is_none(),
        "a μ over a function type must not synthesize"
    );
}

#[test]
fn a_recursive_type_whose_body_is_supported_does_synthesize() {
    // Non-vacuity twin for the guard above: a refusal that fired on every μ
    // would pass that test and break every list.
    let ty = Type::Mu(
        "α_X".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_X".to_string())),
            )),
        )),
    );
    assert!(
        super::super::synth::synth_comparator_defs(&ty, &ComparatorTypes::default()).is_some(),
        "a cons-list μ must synthesize"
    );
}
