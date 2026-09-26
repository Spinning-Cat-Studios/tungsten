//! D2's second half: parameter strictness is COMPUTED, not assumed.
//!
//! The fixpoint that assigns each named type's parameters an `Occ`, the
//! three-way argument dispatch it feeds, and the `via` chain a violation
//! inherited through a forbidden parameter carries.

use crate::types::Type;

use super::{adt, env, tv, violations};
use crate::types::positivity::*;
// D2: parameter strictness is computed, not assumed
// ─────────────────────────────────────────────────────────────────────────

fn fn1() -> (String, PositivityDef) {
    // type Fn1<T> = Mk(T -> Nat)
    adt(
        "Fn1",
        &["T"],
        vec![("Mk", vec![Type::arrow(tv("T"), Type::Nat)])],
    )
}

#[test]
fn fixpoint_marks_an_argument_position_forbidden() {
    let defs = env(vec![fn1()]);
    let occs = param_occurrences(&defs);
    assert_eq!(occs["Fn1"], vec![Occ::Forbidden]);
}

#[test]
fn violation_is_inherited_through_a_forbidden_parameter() {
    // type Bad2 = B(Fn1<Bad2>) — Bad2 is never syntactically left of an arrow.
    let defs = env(vec![
        fn1(),
        adt(
            "Bad2",
            &[],
            vec![("B", vec![Type::app("Fn1", vec![tv("@Bad2")])])],
        ),
    ]);
    let found = violations(&defs, &["Bad2"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].occurrence, "Bad2");
    assert_eq!(
        found[0].via,
        vec![ViaLink {
            type_name: "Fn1".to_string(),
            param: "T".to_string(),
        }],
        "the diagnostic must name the intermediate type and parameter"
    );
}

#[test]
fn fixpoint_propagates_forbidden_through_a_chain() {
    // type Fn2<U> = M(Fn1<U>) — U inherits Fn1's forbidden T.
    let defs = env(vec![
        fn1(),
        adt(
            "Fn2",
            &["U"],
            vec![("M", vec![Type::app("Fn1", vec![tv("U")])])],
        ),
    ]);
    let occs = param_occurrences(&defs);
    assert_eq!(occs["Fn2"], vec![Occ::Forbidden]);
}

#[test]
fn list_parameter_computes_strict_and_nesting_is_accepted() {
    // type List<T> = Nil | Cons(T, List<T>)
    // type Tree = Node(List<Tree>)
    let defs = env(vec![
        adt(
            "List",
            &["T"],
            vec![
                ("Nil", vec![]),
                ("Cons", vec![tv("T"), Type::app("List", vec![tv("T")])]),
            ],
        ),
        adt(
            "Tree",
            &[],
            vec![("Node", vec![Type::app("List", vec![tv("@Tree")])])],
        ),
    ]);
    let occs = param_occurrences(&defs);
    assert_eq!(occs["List"], vec![Occ::Strict]);
    assert!(violations(&defs, &["List"]).is_empty());
    assert!(violations(&defs, &["Tree"]).is_empty());
}

// ─────────────────────────────────────────────────────────────────────────
// D2: a discarded parameter must not be walked at all
// ─────────────────────────────────────────────────────────────────────────

fn phantom() -> (String, PositivityDef) {
    // type Phantom<T> = P(Nat)
    adt("Phantom", &["T"], vec![("P", vec![Type::Nat])])
}

#[test]
fn phantom_parameter_computes_unused() {
    let defs = env(vec![phantom()]);
    assert_eq!(param_occurrences(&defs)["Phantom"], vec![Occ::Unused]);
}

#[test]
fn discarded_argument_is_not_walked_at_any_mode() {
    // type X = MkX(Phantom<X>)          — passes under any reading of Unused
    // type Y = MkY(Phantom<Y -> Nat>)   — rejected by BOTH wrong readings
    let defs = env(vec![
        phantom(),
        adt(
            "X",
            &[],
            vec![("MkX", vec![Type::app("Phantom", vec![tv("@X")])])],
        ),
        adt(
            "Y",
            &[],
            vec![(
                "MkY",
                vec![Type::app("Phantom", vec![Type::arrow(tv("@Y"), Type::Nat)])],
            )],
        ),
    ]);
    assert!(violations(&defs, &["X"]).is_empty());
    assert!(
        violations(&defs, &["Y"]).is_empty(),
        "a discarded argument contains no occurrence at all"
    );
}

// ─────────────────────────────────────────────────────────────────────────
#[test]
fn defs_expose_their_size() {
    let empty = env(vec![]);
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    let one = env(vec![phantom()]);
    assert_eq!(one.len(), 1);
    assert!(!one.is_empty());
    assert_eq!(one.names().collect::<Vec<_>>(), vec!["Phantom"]);
}

// ─────────────────────────────────────────────────────────────────────────
// The `@`-prefix, in both directions
// ─────────────────────────────────────────────────────────────────────────
//
// `Display` strips the `@`, so `TyVar("@List")` and `TyVar("List")` print
// identically while only one is a map key. A binder can therefore be recorded
// under either spelling, and matching only one of them turns a *bound* variable
// back into an occurrence — a false rejection with no escape hatch.

#[test]
fn an_at_prefixed_binder_binds_an_at_prefixed_reference() {
    // μ@A. (@A -> Nat): binder and reference agree verbatim, and the
    // @-stripped forms do NOT (`@A` != `A`).
    let defs = env(vec![adt(
        "A",
        &[],
        vec![(
            "MkA",
            vec![Type::mu("@A".to_string(), Type::arrow(tv("@A"), Type::Nat))],
        )],
    )]);
    assert!(violations(&defs, &["A"]).is_empty());
}

#[test]
fn a_bare_binder_binds_an_at_prefixed_reference() {
    // μA. (@A -> Nat): they agree only after stripping the `@`.
    let defs = env(vec![adt(
        "A",
        &[],
        vec![(
            "MkA",
            vec![Type::mu("A".to_string(), Type::arrow(tv("@A"), Type::Nat))],
        )],
    )]);
    assert!(violations(&defs, &["A"]).is_empty());
}

#[test]
fn an_at_prefixed_reference_still_matches_a_group_member() {
    // The other polarity of the same comparison: with no binder in scope,
    // `@A` must still be recognised as a reference to `A`.
    let defs = env(vec![adt(
        "A",
        &[],
        vec![("MkA", vec![Type::arrow(tv("@A"), Type::Nat)])],
    )]);
    assert_eq!(violations(&defs, &["A"]).len(), 1);
}

#[test]
fn an_already_forbidden_walk_records_no_inherited_link() {
    // `Bad` sits under an arrow domain AND inside `Fn1`, whose parameter is
    // forbidden. The violation is direct — the `via` chain records only the
    // step that ESCALATED a strict walk, so a second, redundant link here
    // would misattribute the cause to `Fn1`.
    let defs = env(vec![
        fn1(),
        adt(
            "Bad",
            &[],
            vec![(
                "Mk",
                vec![Type::arrow(Type::app("Fn1", vec![tv("@Bad")]), Type::Nat)],
            )],
        ),
    ]);
    let found = violations(&defs, &["Bad"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].via.is_empty(),
        "already forbidden: no escalation to attribute — {:?}",
        found[0].via
    );
}

#[test]
fn the_fixpoint_iterates_until_nothing_widens() {
    // `Alpha` is visited BEFORE `Zeta` (definitions are iterated in sorted
    // order), so round 1 leaves `Alpha`'s parameter `Unused` — `Zeta` has not
    // yet been marked. Only a second round propagates it. A fixpoint that
    // stopped after the first widening round would report `Alpha<unused>`,
    // which is a false ACCEPT: the argument would then never be walked.
    let defs = env(vec![
        adt(
            "Alpha",
            &["T"],
            vec![("MkAlpha", vec![Type::app("Zeta", vec![tv("T")])])],
        ),
        adt(
            "Zeta",
            &["U"],
            vec![("MkZeta", vec![Type::arrow(tv("U"), Type::Nat)])],
        ),
    ]);
    let occs = param_occurrences(&defs);
    assert_eq!(occs["Zeta"], vec![Occ::Forbidden]);
    assert_eq!(
        occs["Alpha"],
        vec![Occ::Forbidden],
        "second-round propagation lost"
    );
}
