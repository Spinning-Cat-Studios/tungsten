//! The layer beneath the rule: the SCC collector the walker depends on, the
//! classification of absent heads, and alias expansion.

use std::collections::BTreeMap;

use crate::terms::Term;
use crate::types::Type;

use super::{adt, env, env_with_aliases, tv, violations};
use crate::types::positivity::*;

// D3: the collector traverses a superset of the walker
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn eq_witness_terms_contribute_graph_edges() {
    // The only edge A → B runs through an `Eq` witness term's annotation.
    // `Type::children()` drops those by design, so a collector reusing it
    // splits the SCC — and a split SCC is a false accept.
    let witness = Term::Annot(Box::new(Term::Unit), tv("@B"));
    let defs = env(vec![
        adt(
            "A",
            &[],
            vec![(
                "MkA",
                vec![Type::Eq(
                    Box::new(Type::Unit),
                    Box::new(witness.clone()),
                    Box::new(witness),
                )],
            )],
        ),
        adt("B", &[], vec![("MkB", vec![tv("@A")])]),
    ]);
    let edges = referenced_names(&defs);
    assert!(
        edges["A"].contains("B"),
        "edge through an Eq witness must exist: {edges:?}"
    );
}

#[test]
fn eq_witness_occurrence_is_forbidden() {
    let witness = Term::Annot(Box::new(Term::Unit), tv("@A"));
    let defs = env(vec![adt(
        "A",
        &[],
        vec![(
            "MkA",
            vec![Type::Eq(
                Box::new(Type::Unit),
                Box::new(witness.clone()),
                Box::new(witness),
            )],
        )],
    )]);
    let found = violations(&defs, &["A"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].occurrence, "A");
}

#[test]
fn binders_and_parameters_do_not_record_edges() {
    // `T` is both a parameter of `Holder` and the name of a definition; the
    // shadowed parameter must not weld the two into one SCC.
    let defs = env(vec![
        adt("Holder", &["T"], vec![("H", vec![tv("T")])]),
        adt("T", &[], vec![("MkT", vec![Type::Nat])]),
    ]);
    let edges = referenced_names(&defs);
    assert!(
        !edges["Holder"].contains("T"),
        "a shadowing parameter is not a reference: {edges:?}"
    );

    // Same for a `Mu`/`Forall` binder.
    let bound = env(vec![
        adt(
            "Holder",
            &[],
            vec![("H", vec![Type::mu("T".to_string(), tv("T"))])],
        ),
        adt("T", &[], vec![("MkT", vec![Type::Nat])]),
    ]);
    assert!(!referenced_names(&bound)["Holder"].contains("T"));
}

#[test]
fn mu_bound_variable_is_not_an_occurrence() {
    // μα_A. (α_A -> Nat): the binder is bound, not a reference to `A`.
    let defs = env(vec![adt(
        "A",
        &[],
        vec![(
            "MkA",
            vec![Type::mu("A".to_string(), Type::arrow(tv("A"), Type::Nat))],
        )],
    )]);
    assert!(violations(&defs, &["A"]).is_empty());
}

// ─────────────────────────────────────────────────────────────────────────
// D5: doubts, stubs and poison
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_unknown_head_forbids_its_arguments() {
    // `Unknown` is absent from defs: reject on doubt.
    let defs = env(vec![adt(
        "A",
        &[],
        vec![("MkA", vec![Type::app("Unknown", vec![tv("@A")])])],
    )]);
    let found = violations(&defs, &["A"]);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_stub_head_is_skipped_not_doubted() {
    // A cross-module stub's field types are lossy (`Type::Unit`), so escalating
    // its arguments would report on discarded information — a false rejection
    // on every cross-module reference.
    let defs = PositivityDefs::new(
        vec![adt(
            "A",
            &[],
            vec![("MkA", vec![Type::app("StubGeneric", vec![tv("@A")])])],
        )]
        .into_iter()
        .collect(),
        &BTreeMap::new(),
        ["StubGeneric".to_string()].into_iter().collect(),
    );
    assert!(defs.is_stub("StubGeneric"));
    assert!(
        violations(&defs, &["A"]).is_empty(),
        "a field naming a stub must emit no violation"
    );
}

#[test]
fn error_type_emits_no_violation() {
    // Poison: a positivity diagnostic on an already-failed type is the cascade
    // ADR 7.8.26d exists to prevent.
    let defs = env(vec![adt(
        "A",
        &[],
        vec![("MkA", vec![Type::arrow(Type::Error, tv("@A"))])],
    )]);
    assert!(violations(&defs, &["A"]).is_empty());
}

#[test]
fn ref_and_ptr_contents_are_invariant() {
    for wrap in [Type::ref_ty as fn(Type) -> Type, Type::ptr] {
        let defs = env(vec![adt("A", &[], vec![("MkA", vec![wrap(tv("@A"))])])]);
        let found = violations(&defs, &["A"]);
        assert_eq!(found.len(), 1, "{found:?}");
    }
}

// ─────────────────────────────────────────────────────────────────────────
// D9: alias expansion
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn alias_interposition_is_attributed_to_the_constructor() {
    // type F<T> = T -> Nat ; type Bad4 = B(F<Bad4>)
    let defs = env_with_aliases(
        vec![adt(
            "Bad4",
            &[],
            vec![("B", vec![Type::app("F", vec![tv("@Bad4")])])],
        )],
        vec![("F", vec!["T"], Type::arrow(tv("T"), Type::Nat))],
    );
    let found = violations(&defs, &["Bad4"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].type_name, "Bad4");
    assert_eq!(found[0].ctor_name, "B", "not attributed to the alias");
    assert!(
        found[0].via.is_empty(),
        "inlining makes this a direct arrow-domain occurrence"
    );
}

#[test]
fn phantom_alias_erases_the_occurrence() {
    // The alias counterpart of the `Unused` rule (D2): inlining `type P<T> = Nat`
    // discards the argument, so `MkY(P<Y -> Nat>)` must be accepted — the two
    // rules have to agree.
    let defs = env_with_aliases(
        vec![adt(
            "Y",
            &[],
            vec![(
                "MkY",
                vec![Type::app("P", vec![Type::arrow(tv("@Y"), Type::Nat)])],
            )],
        )],
        vec![("P", vec!["T"], Type::Nat)],
    );
    assert!(violations(&defs, &["Y"]).is_empty());
}

#[test]
fn a_cyclic_alias_expands_to_error_and_emits_nothing() {
    // Alias cycles are E0060 `RecursiveAlias`'s job. The expander bails to
    // `Type::Error`, which is skipped — so no E0061 stacks on top.
    let defs = env_with_aliases(
        vec![adt("A", &[], vec![("MkA", vec![Type::app("Cyc", vec![])])])],
        vec![("Cyc", vec![], Type::app("Cyc", vec![]))],
    );
    assert!(violations(&defs, &["A"]).is_empty());
}

#[test]
fn an_alias_arity_mismatch_expands_to_error() {
    let defs = env_with_aliases(
        vec![adt(
            "A",
            &[],
            vec![("MkA", vec![Type::app("F", vec![tv("@A"), Type::Nat])])],
        )],
        vec![("F", vec!["T"], Type::arrow(tv("T"), Type::Nat))],
    );
    assert!(
        violations(&defs, &["A"]).is_empty(),
        "arity is not this check's error to raise"
    );
}

#[test]
fn a_nullary_alias_is_inlined() {
    // type Alias = A -> Nat ; type A = MkA(Alias)
    let defs = env_with_aliases(
        vec![adt("A", &[], vec![("MkA", vec![tv("Alias")])])],
        vec![("Alias", vec![], Type::arrow(tv("@A"), Type::Nat))],
    );
    let found = violations(&defs, &["A"]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].occurrence, "A");
}

// ─────────────────────────────────────────────────────────────────────────
// The collector's own comparisons
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_app_head_records_an_edge() {
    // `App`/`Adt` heads take a different path from a bare `TyVar`: they are
    // censused as well as recorded, so a graph built only from `TyVar`s would
    // miss every generic instantiation.
    let defs = env(vec![
        adt(
            "A",
            &[],
            vec![("MkA", vec![Type::app("B", vec![Type::Nat])])],
        ),
        adt("B", &["T"], vec![("MkB", vec![tv("T")])]),
    ]);
    assert!(referenced_names(&defs)["A"].contains("B"));
}

#[test]
fn an_adt_head_records_an_edge_and_its_payloads_are_traversed() {
    // An inlined `Adt` carries its variant payloads; the collector must walk
    // them even though the mode-aware walker skips them for a known head.
    let defs = env(vec![
        adt(
            "A",
            &[],
            vec![(
                "MkA",
                vec![Type::adt(
                    "B".to_string(),
                    vec![],
                    vec![("MkB".to_string(), tv("@C"))],
                )],
            )],
        ),
        adt("B", &[], vec![("MkB", vec![Type::Nat])]),
        adt("C", &[], vec![("MkC", vec![Type::Nat])]),
    ]);
    let edges = referenced_names(&defs);
    assert!(edges["A"].contains("B"), "{edges:?}");
    assert!(edges["A"].contains("C"), "payload not traversed: {edges:?}");
}

#[test]
fn an_at_prefixed_binder_records_no_edge() {
    // The collector resolves the same three `TyVar` roles as the walker, and
    // must agree with it on both spellings of a binder — an unfiltered binder
    // yields a spurious edge, hence an over-wide SCC and a false rejection.
    let at_bound = env(vec![
        adt(
            "Holder",
            &[],
            vec![("H", vec![Type::mu("@T".to_string(), tv("@T"))])],
        ),
        adt("T", &[], vec![("MkT", vec![Type::Nat])]),
    ]);
    assert!(!referenced_names(&at_bound)["Holder"].contains("T"));

    let bare_bound = env(vec![
        adt(
            "Holder",
            &[],
            vec![("H", vec![Type::mu("T".to_string(), tv("@T"))])],
        ),
        adt("T", &[], vec![("MkT", vec![Type::Nat])]),
    ]);
    assert!(!referenced_names(&bare_bound)["Holder"].contains("T"));
}

#[test]
fn an_unbound_at_prefixed_reference_still_records_an_edge() {
    // The other polarity: with no binder in scope, `@T` IS a reference to `T`.
    let defs = env(vec![
        adt("Holder", &[], vec![("H", vec![tv("@T")])]),
        adt("T", &[], vec![("MkT", vec![Type::Nat])]),
    ]);
    assert!(referenced_names(&defs)["Holder"].contains("T"));
}

#[test]
fn the_head_census_separates_stubs_from_unknowns() {
    // The two have different fixes, so the census must not merge them — and a
    // resolvable head must appear in neither bucket.
    let defs = PositivityDefs::new(
        vec![adt(
            "A",
            &[],
            vec![(
                "MkA",
                vec![
                    Type::app("Opaque", vec![Type::Nat]),
                    Type::app("Elsewhere", vec![Type::Nat]),
                    Type::app("A", vec![]),
                ],
            )],
        )]
        .into_iter()
        .collect(),
        &BTreeMap::new(),
        ["Opaque".to_string()].into_iter().collect(),
    );
    let census = head_census(&defs);
    assert_eq!(census.stub, ["Opaque".to_string()].into_iter().collect());
    assert_eq!(
        census.unknown,
        ["Elsewhere".to_string()].into_iter().collect()
    );
}

#[test]
fn a_bare_tyvar_head_is_never_censused() {
    // `TyVar` heads carry no arguments, so D5's doubt arm cannot fire on one;
    // censusing them would report noise on every unresolved type parameter.
    let defs = env(vec![adt("A", &[], vec![("MkA", vec![tv("Elsewhere")])])]);
    assert_eq!(head_census(&defs), HeadCensus::default());
}
