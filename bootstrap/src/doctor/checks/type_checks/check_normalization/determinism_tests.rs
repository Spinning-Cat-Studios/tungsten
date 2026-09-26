//! Tests for the cross-run encoding-determinism check (ADR 22.7.26c close-out).
//!
//! The `diff_encoding_maps` comparator is pure, so it is unit-tested directly
//! (no elaboration) to assert the three outcomes: agreement, unequal-tree
//! divergence, and present-in-one-only divergence.

use super::*;
use std::collections::HashMap;
use tungsten_core::Type;

fn map(entries: &[(&str, Type)]) -> HashMap<String, Type> {
    entries
        .iter()
        .map(|(name, ty)| ((*name).to_string(), ty.clone()))
        .collect()
}

#[test]
fn identical_maps_have_no_divergence() {
    let a = map(&[("A", Type::Nat), ("B", Type::sum(Type::Unit, Type::Nat))]);
    let b = a.clone();
    assert!(diff_encoding_maps(&a, &b).is_empty());
}

#[test]
fn unequal_tree_is_divergent_with_both_sides() {
    let a = map(&[("A", Type::sum(Type::Unit, Type::Nat))]);
    let b = map(&[("A", Type::sum(Type::Unit, Type::Bool))]);
    let divergences = diff_encoding_maps(&a, &b);
    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].name, "A");
    assert!(divergences[0].run_a.is_some());
    assert!(divergences[0].run_b.is_some());
}

#[test]
fn present_in_one_run_only_is_divergent() {
    let a = map(&[("A", Type::Nat), ("Only", Type::Bool)]);
    let b = map(&[("A", Type::Nat)]);
    let divergences = diff_encoding_maps(&a, &b);
    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].name, "Only");
    assert!(divergences[0].run_a.is_some());
    assert!(divergences[0].run_b.is_none());
}

#[test]
fn union_len_counts_distinct_names_across_both_runs() {
    // Asymmetric on purpose: run_b has ONE shared key and TWO b-only keys, so
    // |b ∩ a| (1) ≠ |b \ a| (2). This distinguishes the correct
    // `run_a.len() + |b \ a|` = 1 + 2 = 3 from the `!`-deleted mutant's
    // `run_a.len() + |b ∩ a|` = 1 + 1 = 2.
    let a = map(&[("Shared", Type::Nat)]);
    let b = map(&[
        ("Shared", Type::Nat),
        ("OnlyB1", Type::Nat),
        ("OnlyB2", Type::Nat),
    ]);
    assert_eq!(union_len(&a, &b), 3);
    // And the stable count (union − divergences) stays non-negative: the two
    // b-only names diverge, leaving 1 stable ("Shared").
    let divergences = diff_encoding_maps(&a, &b);
    assert_eq!(stable_count(union_len(&a, &b), &divergences), 1);
}

#[test]
fn stable_count_is_total_minus_divergences() {
    // Distinct non-1 values so no constant-return mutant (→0 / →1) survives:
    // the format tests all happen to have stable == 1, which a `-> 1` mutant
    // passes, so stable_count needs its own varied-value coverage.
    let two = vec![
        Divergence {
            name: "A".into(),
            run_a: Some("x".into()),
            run_b: None,
        },
        Divergence {
            name: "B".into(),
            run_a: Some("y".into()),
            run_b: None,
        },
    ];
    assert_eq!(stable_count(2, &two), 0); // all diverged
    assert_eq!(stable_count(5, &two), 3); // non-1 positive
    assert_eq!(stable_count(0, &[]), 0); // empty
}

#[test]
fn format_report_human_lists_divergences_and_stable_count() {
    let a = map(&[("Ok", Type::Nat), ("Bad", Type::sum(Type::Unit, Type::Nat))]);
    let b = map(&[
        ("Ok", Type::Nat),
        ("Bad", Type::sum(Type::Unit, Type::Bool)),
    ]);
    let divergences = diff_encoding_maps(&a, &b);
    let out = format_report(
        union_len(&a, &b),
        &divergences,
        false,
        /* json */ false,
    );
    assert!(out.contains("cross-run determinism"), "human header: {out}");
    assert!(
        out.contains("Bad: NON-DETERMINISTIC"),
        "names the divergent type: {out}"
    );
    assert!(
        out.contains("1 non-deterministic, 1 stable"),
        "reports the tally: {out}"
    );
}

#[test]
fn format_report_json_dispatch_emits_json_not_human() {
    let a = map(&[("Ok", Type::Nat)]);
    let b = map(&[("Ok", Type::Nat)]);
    let divergences = diff_encoding_maps(&a, &b);
    let out = format_report(union_len(&a, &b), &divergences, false, /* json */ true);
    // The `json` dispatch must pick the JSON formatter, not the human one.
    assert!(out.trim_start().starts_with('{'), "must be JSON: {out}");
    assert!(
        out.contains("\"non_deterministic\": 0"),
        "stable JSON: {out}"
    );
    assert!(out.contains("\"stable\": 1"), "stable count in JSON: {out}");
    assert!(
        !out.contains("cross-run determinism"),
        "must not be the human header: {out}"
    );
}

#[test]
fn divergences_are_sorted_by_name() {
    let a = map(&[("Zeta", Type::Nat), ("Alpha", Type::Nat), ("Mu", Type::Nat)]);
    let b = map(&[
        ("Zeta", Type::Bool),
        ("Alpha", Type::Bool),
        ("Mu", Type::Bool),
    ]);
    let divergences = diff_encoding_maps(&a, &b);
    let names: Vec<&str> = divergences.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Mu", "Zeta"]);
}
