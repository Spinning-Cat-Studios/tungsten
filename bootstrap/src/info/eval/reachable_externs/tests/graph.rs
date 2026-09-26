//! The Core call-graph walk: what it reaches, in what order, and what it
//! refuses to pretend it knows.

use tungsten_core::{Term, Type};

use super::*;

#[test]
fn analyze_returns_none_for_a_definition_that_does_not_exist() {
    let globals = project(&[("main", Term::Unit)]);
    assert!(analyze_default(&globals, "absent").is_none());
}

#[test]
fn a_definition_reaching_nothing_reports_one_def_walked_and_no_externs() {
    let globals = project(&[("main", Term::Unit)]);
    let report = analyze_default(&globals, "main").unwrap();

    // The distinction the command exists to keep: examined-one-and-found-none
    // must not be representable as the same state as examined-nothing.
    assert_eq!(report.defs_visited, 1);
    assert!(report.reached.is_empty());
    assert_eq!(report.blocking().count(), 0);
    assert!(report.complete());
}

#[test]
fn a_direct_extern_call_is_reached_with_a_one_element_chain() {
    let globals = project(&[("shout", extern_wrapper("tg_println"))]);
    let report = analyze_default(&globals, "shout").unwrap();

    assert_eq!(report.reached.len(), 1);
    assert_eq!(report.reached[0].symbol, "tg_println");
    assert!(report.reached[0].executable);
    assert_eq!(report.reached[0].via, vec!["shout".to_string()]);
}

#[test]
fn the_c_abi_prefix_is_stripped_before_the_registry_is_consulted() {
    // The elaborator emits `__c_tg_println`; the evaluator strips the prefix
    // before dispatch. Matching the raw symbol would report every extern
    // unexecutable — a report that is wrong in the safe-looking direction.
    let globals = project(&[("shout", extern_wrapper("__c_tg_println"))]);
    let report = analyze_default(&globals, "shout").unwrap();

    assert_eq!(report.reached[0].symbol, "tg_println");
    assert!(report.reached[0].executable);
}

#[test]
fn an_unexecutable_extern_is_reported_as_blocking() {
    let globals = project(&[("joiner", extern_wrapper("tg_path_join"))]);
    let report = analyze_default(&globals, "joiner").unwrap();

    assert_eq!(report.blocking().count(), 1);
    assert_eq!(report.blocking().next().unwrap().symbol, "tg_path_join");
    assert!(!report.reached[0].executable);
}

#[test]
fn an_extern_reached_through_two_hops_records_the_whole_chain() {
    // The ADR 7.8.26a case: the tested function looked pure, and the
    // unexecutable extern was two calls away.
    let globals = project(&[
        ("harness_path", calls("path_join")),
        ("path_join", calls("tg_path_join")),
        ("tg_path_join", extern_wrapper("__c_tg_path_join")),
    ]);
    let report = analyze_default(&globals, "harness_path").unwrap();

    assert_eq!(report.defs_visited, 3);
    assert_eq!(report.reached.len(), 1);
    assert_eq!(
        report.reached[0].via,
        vec![
            "harness_path".to_string(),
            "path_join".to_string(),
            "tg_path_join".to_string()
        ]
    );
}

#[test]
fn the_recorded_chain_is_the_shortest_one() {
    // `tg_println` is reachable directly and via a detour. Breadth-first order
    // is what makes the reported chain actionable rather than merely true.
    let globals = project(&[
        (
            "root",
            Term::Pair(Box::new(calls("direct")), Box::new(calls("scenic"))),
        ),
        ("scenic", calls("longer")),
        ("longer", calls("direct")),
        ("direct", extern_wrapper("tg_println")),
    ]);
    let report = analyze_default(&globals, "root").unwrap();

    assert_eq!(report.reached.len(), 1);
    assert_eq!(
        report.reached[0].via,
        vec!["root".to_string(), "direct".to_string()]
    );
}

#[test]
fn a_recursive_definition_terminates_and_is_counted_once() {
    let globals = project(&[
        (
            "loop_a",
            Term::Pair(
                Box::new(calls("loop_b")),
                Box::new(extern_wrapper("tg_path_join")),
            ),
        ),
        ("loop_b", calls("loop_a")),
    ]);
    let report = analyze_default(&globals, "loop_a").unwrap();

    assert_eq!(report.defs_visited, 2);
    assert_eq!(report.blocking().count(), 1);
}

#[test]
fn a_global_with_no_definition_is_reported_rather_than_ignored() {
    let globals = project(&[("main", calls("nowhere"))]);
    let report = analyze_default(&globals, "main").unwrap();

    assert_eq!(report.unresolved, vec!["nowhere".to_string()]);
    assert_eq!(report.defs_visited, 1);
}

#[test]
fn externs_are_found_inside_nested_terms_not_just_at_the_head() {
    // The walk must reach an extern buried in a lambda body inside a let
    // binding inside a match arm — a shallow walk would report a false clean.
    let buried = Term::Let(
        "x".to_string(),
        Type::Unit,
        Box::new(Term::Unit),
        Box::new(Term::Lambda(
            "y".to_string(),
            Type::Unit,
            Box::new(Term::If(
                Box::new(Term::True),
                Box::new(extern_wrapper("tg_path_join")),
                Box::new(Term::Unit),
            )),
        )),
    );
    let globals = project(&[("deep", buried)]);
    let report = analyze_default(&globals, "deep").unwrap();

    assert_eq!(report.blocking().count(), 1);
}

#[test]
fn blocking_externs_sort_before_executable_ones() {
    // `tg_println` alphabetically follows `tg_path_join`, so a stable-by-name
    // sort alone would not prove the executable/blocking key is applied first.
    let globals = project(&[(
        "both",
        Term::Pair(
            Box::new(extern_wrapper("tg_println")),
            Box::new(extern_wrapper("tg_path_join")),
        ),
    )]);
    let report = analyze_default(&globals, "both").unwrap();

    assert_eq!(report.reached.len(), 2);
    assert_eq!(report.reached[0].symbol, "tg_path_join");
    assert!(!report.reached[0].executable);
    assert!(report.reached[1].executable);
}
