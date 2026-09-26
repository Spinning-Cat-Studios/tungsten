//! The occurrence graph's call/opaque split, and the SCCs built over it.

use std::collections::BTreeMap;

use crate::terms::termination::graph::{
    is_recursive, peel_spine, tarjan_scc, transparent, OccurrenceGraph,
};
use crate::terms::{Term, TermSpan};
use crate::types::Type;

use super::fixtures::{call, var};

fn graph_of(defs: &[(&str, Term)]) -> OccurrenceGraph {
    OccurrenceGraph::build(defs.iter().map(|(name, term)| (*name, term)))
}

#[test]
fn call_position_and_value_position_are_distinguished() {
    let called = call("g", vec![var("x")]);
    let stored = Term::Let(
        "h".to_string(),
        Type::Nat,
        Box::new(Term::Global("g".to_string())),
        Box::new(var("h")),
    );
    let graph = graph_of(&[("f", called), ("k", stored), ("g", Term::Zero)]);

    assert!(graph.calls("f").unwrap().contains("g"));
    assert!(graph.opaque_uses("f").unwrap().is_empty());
    assert!(graph.opaque_uses("k").unwrap().contains("g"));
    assert!(graph.calls("k").unwrap().is_empty());
}

#[test]
fn both_edge_kinds_reach_the_union_view() {
    let term = Term::Pair(
        Box::new(call("g", vec![Term::Zero])),
        Box::new(Term::Global("h".to_string())),
    );
    let graph = graph_of(&[("f", term), ("g", Term::Zero), ("h", Term::Zero)]);

    assert_eq!(
        graph.mentions("f"),
        ["g".to_string(), "h".to_string()].into()
    );
    assert!(graph.has_edge("f", "g"));
    assert!(graph.has_edge("f", "h"));
    assert!(!graph.has_edge("g", "f"));
    assert_eq!(graph.node_count(), 3);
    assert_eq!(graph.edge_count(), 2);
}

#[test]
fn unknown_globals_are_not_nodes() {
    let term = call("not_a_definition", vec![Term::Zero]);
    let graph = graph_of(&[("f", term)]);

    assert!(graph.calls("f").unwrap().is_empty());
    assert_eq!(graph.node_count(), 1);
}

#[test]
fn spans_and_annotations_are_transparent_to_the_split() {
    let term = Term::Spanned(
        Box::new(Term::Annot(
            Box::new(call("g", vec![Term::Zero])),
            Type::Nat,
        )),
        TermSpan::new(0, 1),
    );
    let graph = graph_of(&[("f", term), ("g", Term::Zero)]);

    assert!(graph.calls("f").unwrap().contains("g"));
}

#[test]
fn type_application_alone_is_a_value_occurrence() {
    // `g[Nat]` supplies no value argument, so `g` escapes as a closure.
    let term = Term::TyApp(Box::new(Term::Global("g".to_string())), Type::Nat);
    let graph = graph_of(&[("f", term), ("g", Term::Zero)]);

    assert!(graph.opaque_uses("f").unwrap().contains("g"));
    assert!(graph.calls("f").unwrap().is_empty());
}

#[test]
fn type_application_under_a_call_still_supplies_arguments_from_zero() {
    let term = Term::App(
        Box::new(Term::TyApp(
            Box::new(Term::Global("g".to_string())),
            Type::Nat,
        )),
        Box::new(var("x")),
    );
    let (head, args) = peel_spine(&term);

    assert_eq!(head, &Term::Global("g".to_string()));
    assert_eq!(args.len(), 1);
    assert_eq!(transparent(args[0]), &var("x"));
}

#[test]
fn a_singleton_without_a_self_edge_is_not_recursive() {
    let graph = graph_of(&[("f", call("g", vec![Term::Zero])), ("g", Term::Zero)]);
    let adjacency = graph.adjacency();

    for component in tarjan_scc(&adjacency) {
        assert!(!is_recursive(&component, &adjacency), "{component:?}");
    }
}

#[test]
fn self_edges_and_mutual_cycles_are_recursive() {
    let graph = graph_of(&[
        ("f", call("f", vec![Term::Zero])),
        ("a", call("b", vec![Term::Zero])),
        ("b", call("a", vec![Term::Zero])),
    ]);
    let adjacency = graph.adjacency();
    let recursive: Vec<Vec<String>> = tarjan_scc(&adjacency)
        .into_iter()
        .filter(|component| is_recursive(component, &adjacency))
        .collect();

    assert_eq!(
        recursive,
        vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["f".to_string()]
        ]
    );
}

#[test]
fn an_opaque_cycle_is_still_an_scc() {
    // `let h = f; h(x)` — no call-position occurrence of `f` anywhere.
    let term = Term::Let(
        "h".to_string(),
        Type::Nat,
        Box::new(Term::Global("f".to_string())),
        Box::new(call("h", vec![var("x")])),
    );
    let graph = graph_of(&[("f", term)]);
    let adjacency = graph.adjacency();

    assert!(is_recursive(&["f".to_string()], &adjacency));
}

#[test]
fn components_come_back_in_a_stable_order() {
    let build = || {
        let graph = graph_of(&[
            ("z", call("y", vec![Term::Zero])),
            ("y", call("x", vec![Term::Zero])),
            ("x", Term::Zero),
        ]);
        tarjan_scc(&graph.adjacency())
    };

    assert_eq!(build(), build());
}

#[test]
fn edges_to_names_outside_the_adjacency_are_skipped() {
    let mut adjacency: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    adjacency.insert("f".to_string(), ["ghost".to_string()].into());

    assert_eq!(tarjan_scc(&adjacency), vec![vec!["f".to_string()]]);
}
