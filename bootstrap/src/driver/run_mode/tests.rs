//! Tests for the stuck-residual diagnosis (`run_mode::stuck_diagnosis`).
//!
//! Tests: bootstrap/src/driver/run_mode/mod.rs

use std::collections::HashSet;

use tungsten_core::types::Type;
use tungsten_core::Term;

use super::{stopped_note, stuck_diagnosis};

fn defined(names: &[&str]) -> HashSet<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

// ── a real value is never diagnosed ─────────────────────────────────────────

#[test]
fn a_value_produces_no_diagnosis() {
    assert_eq!(stuck_diagnosis(&Term::Zero, &HashSet::new()), None);
    assert_eq!(
        stuck_diagnosis(&Term::Succ(Box::new(Term::Zero)), &HashSet::new()),
        None
    );
    assert_eq!(stuck_diagnosis(&Term::True, &HashSet::new()), None);
    assert_eq!(
        stuck_diagnosis(&Term::StringLit("x".into()), &HashSet::new()),
        None
    );
}

/// A pair of values is a value — the walk must not report its way into one.
#[test]
fn a_pair_of_values_produces_no_diagnosis() {
    let pair = Term::Pair(Box::new(Term::Zero), Box::new(Term::True));
    assert_eq!(stuck_diagnosis(&pair, &HashSet::new()), None);
}

// ── unresolved globals ──────────────────────────────────────────────────────

#[test]
fn an_undefined_global_is_named() {
    let term = Term::App(
        Box::new(Term::Global("missing_fn".into())),
        Box::new(Term::Zero),
    );
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("missing_fn"), "{msg}");
    assert!(msg.contains("never resolved"), "{msg}");
}

/// Membership is *checked*, not guessed from the name — a global the
/// environment defines must not be blamed.
#[test]
fn a_defined_global_is_not_blamed() {
    let term = Term::App(
        Box::new(Term::Global("compare_Nat".into())),
        Box::new(Term::Zero),
    );
    let msg = stuck_diagnosis(&term, &defined(&["compare_Nat"])).expect("not a value");
    assert!(
        !msg.contains("never resolved"),
        "compare_Nat is defined here: {msg}"
    );
}

#[test]
fn an_undefined_comparator_gets_the_comparator_hint() {
    let term = Term::Global("compare_Foo".into());
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("compare_Foo"), "{msg}");
    assert!(
        msg.contains("doctor check comparable"),
        "must point at the tool that explains it: {msg}"
    );
}

#[test]
fn undefined_globals_are_found_at_depth() {
    let deep = Term::App(
        Box::new(Term::Lambda(
            "x".into(),
            Type::Nat,
            Box::new(Term::Succ(Box::new(Term::Global("buried".into())))),
        )),
        Box::new(Term::Zero),
    );
    let msg = stuck_diagnosis(&deep, &HashSet::new()).expect("not a value");
    assert!(msg.contains("buried"), "{msg}");
}

// ── stuck projections (the constructor-payload nesting mismatch) ────────────

/// `Fst` of a `Nat` is exactly what a left-nested-type / right-nested-value
/// mismatch produces at runtime.
#[test]
fn a_projection_on_a_scalar_is_diagnosed_as_a_nesting_mismatch() {
    let term = Term::Fst(Box::new(Term::Succ(Box::new(Term::Zero))));
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("Fst"), "{msg}");
    assert!(msg.contains("non-pair"), "{msg}");
    assert!(
        msg.contains("adt-abi-safety"),
        "must point at the invariant it violates: {msg}"
    );
}

#[test]
fn snd_on_a_scalar_is_also_diagnosed() {
    let term = Term::Snd(Box::new(Term::True));
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("Snd"), "{msg}");
}

/// A projection on a genuine pair is not stuck and must not be blamed.
#[test]
fn a_projection_on_a_pair_is_not_reported() {
    let term = Term::Fst(Box::new(Term::Pair(
        Box::new(Term::Zero),
        Box::new(Term::True),
    )));
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(
        !msg.contains("non-pair"),
        "Fst of a real pair is not the nesting bug: {msg}"
    );
}

/// A stuck projection outranks an unreached global. `Fst` of a scalar can
/// never step; a global in a branch evaluation never entered was simply never
/// forced, and blaming it points the reader at the wrong node — which is
/// exactly what the first cut of this function did on the arity repro.
#[test]
fn a_stuck_projection_takes_precedence_over_an_unreached_global() {
    let term = Term::Pair(
        Box::new(Term::Fst(Box::new(Term::Zero))),
        Box::new(Term::Global("never_forced".into())),
    );
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("non-pair"), "{msg}");
    assert!(!msg.contains("never_forced"), "{msg}");
}

// ── fallback ────────────────────────────────────────────────────────────────

/// Even with no recognizable cause the caller learns it is a residual and what
/// shape it has — strictly more than an unannotated Core IR dump.
#[test]
fn an_unrecognized_stuck_term_still_names_its_head() {
    let term = Term::App(Box::new(Term::Var("unbound".into())), Box::new(Term::Zero));
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("application"), "{msg}");
    assert!(msg.contains("no evaluation rule applies"), "{msg}");
}

// ── child_terms must REACH every subterm ────────────────────────────────
//
// The two searches are only as good as this walk: a container variant with
// no arm silently hides whatever is inside it, and the diagnosis degrades
// to the generic fallback without saying so. Each case buries an undefined
// global one level down and asserts it is still found.

fn buries_global(build: fn(Box<Term>) -> Term) -> bool {
    let term = build(Box::new(Term::Global("buried_marker".into())));
    stuck_diagnosis(&term, &HashSet::new()).is_some_and(|m| m.contains("buried_marker"))
}

/// Guards the helper itself: shapes with nothing reachable must return false,
/// or `every_container_variant_is_walked` passes vacuously (measured survivor:
/// `replace buries_global -> bool with true`).
#[test]
fn buries_global_is_discriminating() {
    // Discards the marker entirely: the result is a value, so no diagnosis.
    assert!(!buries_global(|_| Term::Zero));
    // Keeps a stuck term but not the marker.
    assert!(!buries_global(|_| Term::Var("plain".into())));
    // …and a shape that DOES bury it still reports true.
    assert!(buries_global(|t| Term::App(t, Box::new(Term::Zero))));
}

#[test]
fn every_container_variant_is_walked() {
    let cases: Vec<(&str, fn(Box<Term>) -> Term)> = vec![
        ("App-fn", |t| Term::App(t, Box::new(Term::Zero))),
        ("App-arg", |t| Term::App(Box::new(Term::Zero), t)),
        ("Pair-fst", |t| Term::Pair(t, Box::new(Term::Zero))),
        ("Pair-snd", |t| Term::Pair(Box::new(Term::Zero), t)),
        ("Fst", Term::Fst),
        ("Snd", Term::Snd),
        ("Succ", Term::Succ),
        ("Inl", |t| Term::Inl(Type::Nat, t)),
        ("Inr", |t| Term::Inr(Type::Nat, t)),
        ("Fold", |t| Term::Fold(Type::Nat, t)),
        ("Unfold", |t| Term::Unfold(Type::Nat, t)),
        ("Annot", |t| Term::Annot(t, Type::Nat)),
        ("TyApp", |t| Term::TyApp(t, Type::Nat)),
        ("Let-bound", |t| {
            Term::Let("x".into(), Type::Nat, t, Box::new(Term::Zero))
        }),
        ("Let-body", |t| {
            Term::Let("x".into(), Type::Nat, Box::new(Term::Zero), t)
        }),
        ("If-cond", |t| {
            Term::If(t, Box::new(Term::Zero), Box::new(Term::Zero))
        }),
        ("AdtConstruct", |t| Term::AdtConstruct(Type::Nat, 0, t)),
        ("ExternCall", |t| Term::ExternCall("e".into(), vec![*t])),
    ];
    for (label, build) in cases {
        assert!(buries_global(build), "child_terms does not walk {label}");
    }
}

#[test]
fn case_and_adt_match_arms_are_walked() {
    let in_case = Term::Case(
        Box::new(Term::Zero),
        "l".into(),
        Box::new(Term::Global("buried_marker".into())),
        "r".into(),
        Box::new(Term::Zero),
    );
    assert!(stuck_diagnosis(&in_case, &HashSet::new()).is_some_and(|m| m.contains("buried_marker")));

    let in_match = Term::AdtMatch(
        Box::new(Term::Zero),
        vec![(
            0,
            "v".into(),
            Box::new(Term::Global("buried_marker".into())),
        )],
    );
    assert!(
        stuck_diagnosis(&in_match, &HashSet::new()).is_some_and(|m| m.contains("buried_marker"))
    );
}

#[test]
fn head_label_names_the_shape_for_each_stuck_head() {
    let cases: Vec<(Term, &str)> = vec![
        (
            Term::App(Box::new(Term::Var("f".into())), Box::new(Term::Zero)),
            "application",
        ),
        (Term::Fst(Box::new(Term::Var("p".into()))), "projection"),
        (
            Term::Unfold(Type::Nat, Box::new(Term::Var("x".into()))),
            "unfold",
        ),
        (
            Term::ExternCall("e".into(), vec![Term::Var("a".into())]),
            "extern call",
        ),
    ];
    for (term, want) in cases {
        let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
        assert!(msg.contains(want), "expected {want} in: {msg}");
    }
}

/// A lambda IS a value, so a bare one is never diagnosed however odd its
/// body — evaluation stopped normally, nothing is stuck. This is the
/// correct behaviour and worth pinning: the obvious "walk every container"
/// test asserts the opposite and fails here for the right reason.
#[test]
fn a_lambda_is_a_value_so_its_body_is_not_diagnosed() {
    let lam = Term::Lambda(
        "x".into(),
        Type::Nat,
        Box::new(Term::Global("buried_marker".into())),
    );
    assert_eq!(stuck_diagnosis(&lam, &HashSet::new()), None);
    assert_eq!(
        stuck_diagnosis(
            &Term::TyAbs("T".into(), Box::new(lam.clone())),
            &HashSet::new()
        ),
        None
    );
}

/// ...but the `Lambda`/`TyAbs` arms of `child_terms` still earn their place:
/// once the ENCLOSING term is stuck, the walk must descend through them.
#[test]
fn lambda_bodies_are_reached_when_the_enclosing_term_is_stuck() {
    let stuck_app = Term::App(
        Box::new(Term::Var("unbound".into())),
        Box::new(Term::Lambda(
            "x".into(),
            Type::Nat,
            Box::new(Term::Global("buried_marker".into())),
        )),
    );
    let msg = stuck_diagnosis(&stuck_app, &HashSet::new()).expect("not a value");
    assert!(msg.contains("buried_marker"), "{msg}");
}

/// A NON-comparator global must not receive the comparator hint. Without this,
/// flipping `name == "__cmp"` to `!=` routes every ordinary missing global into
/// the comparator branch and no assertion notices, because both messages
/// contain "never resolved" (measured survivor).
#[test]
fn an_ordinary_global_does_not_get_the_comparator_hint() {
    let term = Term::Global("some_ordinary_fn".into());
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("some_ordinary_fn"), "{msg}");
    assert!(
        !msg.contains("doctor check comparable"),
        "comparator hint leaked onto an ordinary global: {msg}"
    );
}

/// `__cmp` itself is the intrinsic and DOES get the hint — the other side of
/// the same equality.
#[test]
fn the_cmp_intrinsic_gets_the_comparator_hint() {
    let term = Term::Global("__cmp".into());
    let msg = stuck_diagnosis(&term, &HashSet::new()).expect("not a value");
    assert!(msg.contains("doctor check comparable"), "{msg}");
}

// ── the follow-up note per stop (ADR 1.8.26b) ───────────────────────────────

/// Each stop that HAS a next move must name a different one, and the two that
/// do not must stay silent. A single "does it return Some" assertion is
/// satisfied by a function that returns the same string for everything, which
/// would send a reader chasing the wrong diagnostic.
#[test]
fn each_stop_gets_its_own_follow_up_note() {
    use tungsten_core::eval::{ComparatorFailure, ComparatorFailureKind, EvalStopped};

    let black_hole = stopped_note(&EvalStopped::BlackHole {
        cycle: vec!["f".into(), "f".into()],
    })
    .expect("a black hole has a next move");
    let uncomparable = stopped_note(&EvalStopped::Uncomparable(ComparatorFailure::new(
        "Alpha",
        ComparatorFailureKind::EmptyClosure,
    )))
    .expect("an uncomparable type has a next move");
    let never_ran = stopped_note(&EvalStopped::ComparisonNeverRan {
        symbol: "compare_Alpha".into(),
    })
    .expect("a residual comparison has a next move");
    let malformed = stopped_note(&EvalStopped::MalformedElimination {
        eliminator: "Fst",
        head: "a Nat".into(),
    })
    .expect("a representation mismatch has a next move");

    let notes = [black_hole, uncomparable, never_ran, malformed];
    for (i, a) in notes.iter().enumerate() {
        assert!(!a.is_empty(), "an empty note is worse than none");
        for b in notes.iter().skip(i + 1) {
            assert_ne!(a, b, "two stops share a follow-up note");
        }
    }
    assert!(
        uncomparable.contains("doctor check comparable"),
        "the uncomparable note must name the diagnostic: {uncomparable}"
    );

    // The two whose `Display` already says everything add nothing.
    assert_eq!(stopped_note(&EvalStopped::TimedOut { steps: 1 }), None);
    assert_eq!(stopped_note(&EvalStopped::StepLimit { limit: 1 }), None);
}
