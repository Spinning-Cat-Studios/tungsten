//! The strict-subterm relation: what counts as descent and what does not.

use std::collections::HashMap;

use crate::terms::termination::size_env::{classify, collect_call_sites, SizeClass};
use crate::terms::termination::SizeClass as ExportedSizeClass;
use crate::terms::{Term, TermSpan};
use crate::types::Type;

use super::fixtures::{call, list_type, match_list, var};

fn root_env(root: &str) -> HashMap<String, SizeClass> {
    HashMap::from([(root.to_string(), SizeClass::SameAsRoot)])
}

fn smaller_env(name: &str) -> HashMap<String, SizeClass> {
    HashMap::from([(name.to_string(), SizeClass::Smaller)])
}

#[test]
fn the_root_is_not_a_strict_subterm_of_itself() {
    assert_eq!(
        classify(&var("a"), &root_env("a")),
        Some(ExportedSizeClass::SameAsRoot)
    );
}

#[test]
fn unfolding_preserves_size_and_projection_shrinks_it() {
    let unfolded = Term::Unfold(list_type(), Box::new(var("a")));
    assert_eq!(
        classify(&unfolded, &root_env("a")),
        Some(SizeClass::SameAsRoot)
    );

    let projected = Term::Snd(Box::new(unfolded));
    assert_eq!(
        classify(&projected, &root_env("a")),
        Some(SizeClass::Smaller)
    );
}

#[test]
fn reconstruction_is_not_descent() {
    // `Succ(n)` is not a strict subterm of `Succ(n)` — nor is any other
    // constructor applied to something smaller.
    let env = smaller_env("n");
    assert_eq!(classify(&var("n"), &env), Some(SizeClass::Smaller));
    assert_eq!(classify(&Term::Succ(Box::new(var("n"))), &env), None);
    assert_eq!(
        classify(&Term::Fold(list_type(), Box::new(var("n"))), &env),
        None
    );
    assert_eq!(
        classify(&Term::Pair(Box::new(var("n")), Box::new(Term::Zero)), &env),
        None
    );
}

#[test]
fn a_term_unrelated_to_the_root_classifies_as_nothing() {
    assert_eq!(classify(&var("b"), &root_env("a")), None);
    assert_eq!(classify(&Term::Zero, &root_env("a")), None);
    assert_eq!(classify(&call("g", vec![var("a")]), &root_env("a")), None);
}

#[test]
fn matching_the_root_binds_a_strictly_smaller_tail() {
    // fn len(l) = match l { Nil => 0, Cons(_, t) => len(t) }
    let body = match_list(var("l"), Term::Zero, "t", call("len", vec![var("t")]));
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].callee, "len");
    assert!(sites[0].supplies(0));
    assert!(sites[0].descends_at(0));
}

#[test]
fn matching_something_else_binds_nothing_smaller() {
    // The scrutinee is a different parameter, so its payload says nothing
    // about `l`.
    let body = match_list(var("other"), Term::Zero, "t", call("len", vec![var("t")]));
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert_eq!(sites.len(), 1);
    assert!(!sites[0].descends_at(0));
}

#[test]
fn passing_the_root_straight_back_is_not_descent() {
    let body = match_list(var("l"), Term::Zero, "t", call("len", vec![var("l")]));
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert!(!sites[0].descends_at(0));
}

#[test]
fn a_let_alias_of_a_subterm_stays_a_subterm() {
    let aliased = Term::Let(
        "u".to_string(),
        list_type(),
        Box::new(var("t")),
        Box::new(call("len", vec![var("u")])),
    );
    let body = match_list(var("l"), Term::Zero, "t", aliased);
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert!(sites[0].descends_at(0));
}

#[test]
fn rebinding_a_name_clears_what_was_proved_about_it() {
    // `let t = l in len(t)` inside the Cons arm: the inner `t` is the whole
    // list again, so the descent the pattern established is gone.
    let shadowing = Term::Let(
        "t".to_string(),
        list_type(),
        Box::new(var("l")),
        Box::new(call("len", vec![var("t")])),
    );
    let body = match_list(var("l"), Term::Zero, "t", shadowing);
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert!(!sites[0].descends_at(0));
}

#[test]
fn a_lambda_binder_shadows_the_root() {
    let inner = Term::Lambda(
        "l".to_string(),
        list_type(),
        Box::new(call("len", vec![var("l")])),
    );
    let sites = collect_call_sites(&inner, "l", &["len".to_string()]);

    assert!(!sites[0].descends_at(0));
}

#[test]
fn nested_destructuring_descends_transitively() {
    // match l { .., Cons(_, t) => match t { .., Cons(_, u) => len(u) } }
    let inner = match_list(var("t"), Term::Zero, "u", call("len", vec![var("u")]));
    let body = match_list(var("l"), Term::Zero, "t", inner);
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert_eq!(sites.len(), 1);
    assert!(sites[0].descends_at(0));
}

#[test]
fn call_sites_carry_the_enclosing_span_and_rendered_arguments() {
    let spanned = Term::Spanned(
        Box::new(call("len", vec![var("t"), Term::Zero])),
        TermSpan::new(7, 19),
    );
    let body = match_list(var("l"), Term::Zero, "t", spanned);
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert_eq!(sites[0].span, Some(TermSpan::new(7, 19)));
    assert_eq!(sites[0].arguments.len(), 2);
    assert_eq!(sites[0].arguments[0], "t");
    assert!(sites[0].supplies(1));
    assert!(!sites[0].descends_at(1));
}

#[test]
fn calls_outside_the_group_are_not_collected() {
    let body = match_list(var("l"), Term::Zero, "t", call("elsewhere", vec![var("t")]));
    let sites = collect_call_sites(&body, "l", &["len".to_string()]);

    assert!(sites.is_empty());
}

#[test]
fn an_adt_match_binds_its_payload_the_same_way() {
    let adt = Type::Adt(
        "Tri".to_string(),
        vec![],
        vec![
            ("A".to_string(), Type::Unit),
            ("B".to_string(), list_type()),
        ],
    );
    let body = Term::AdtMatch(
        Box::new(var("t")),
        vec![
            (0, "__ctor_A".to_string(), Box::new(Term::Zero)),
            (
                1,
                "__ctor_B".to_string(),
                Box::new(call("tri", vec![var("__ctor_B")])),
            ),
        ],
    );
    let _ = adt;
    let sites = collect_call_sites(&body, "t", &["tri".to_string()]);

    assert!(sites[0].descends_at(0));
}
