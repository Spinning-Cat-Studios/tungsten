//! The ancestor rule, the entry-file convention, and the partition itself —
//! all over hand-built adjacencies, so the whole decision is assertable without
//! a source tree.

use super::*;

// ---------------------------------------------------------------------------
// The ancestor rule
// ---------------------------------------------------------------------------

#[test]
fn a_qualified_module_needs_every_prefix_of_itself() {
    assert_eq!(
        ancestors_of("codegen::ir_closures::lambda"),
        vec!["codegen".to_string(), "codegen::ir_closures".to_string()]
    );
}

#[test]
fn a_top_level_module_has_no_ancestors() {
    assert!(ancestors_of("codegen").is_empty());
}

#[test]
fn reaching_a_child_reaches_its_parent_even_though_nothing_imports_the_parent() {
    // The failure this rule exists to prevent: every `mod.tg` in the tree reads
    // as unreached, because a `use` names a leaf and never its parent.
    let injected = input(
        &["elab", "elab::cir", "elab::cir::types"],
        &[("main.tg", &["elab::cir::types"])],
        "main.tg",
        &[],
    );
    assert_eq!(
        partition_reach(&injected).driver_reached,
        set(&["elab", "elab::cir", "elab::cir::types"])
    );
}

// ---------------------------------------------------------------------------
// The entry-file convention
// ---------------------------------------------------------------------------

#[test]
fn the_runners_two_prefixes_are_entry_files() {
    assert!(is_test_entry_stem("test_codegen"));
    assert!(is_test_entry_stem("mustfail_ast_compare"));
}

#[test]
fn a_stem_merely_starting_with_test_is_not_an_entry_file() {
    assert!(!is_test_entry_stem("tester"));
    assert!(!is_test_entry_stem("testing"));
    assert!(!is_test_entry_stem("main"));
    assert!(!is_test_entry_stem("attest"));
}

// ---------------------------------------------------------------------------
// The partition
// ---------------------------------------------------------------------------

/// ADR 3.9.26a AC4 — the shape `audit-dead-definitions` cannot express: a module
/// reachable from a `test_*` root and from no driver path. Run on the driver
/// entry that census calls it unreachable; run on the test entry it calls it
/// live; neither answer is "the driver never runs this".
#[test]
fn a_module_only_a_test_entry_imports_is_test_only() {
    let injected = input(
        &["driver", "codegen", "codegen::ir_expr"],
        &[
            ("main.tg", &["driver"]),
            ("test_codegen.tg", &["codegen::ir_expr"]),
        ],
        "main.tg",
        &["test_codegen.tg"],
    );
    let reach = partition_reach(&injected);

    assert_eq!(reach.driver_reached, set(&["driver"]));
    assert_eq!(reach.test_only, set(&["codegen", "codegen::ir_expr"]));
    assert!(reach.unreached.is_empty(), "{:?}", reach.unreached);
}

#[test]
fn a_module_both_entries_import_is_driver_reached_not_test_only() {
    let injected = input(
        &["parser"],
        &[("main.tg", &["parser"]), ("test_x.tg", &["parser"])],
        "main.tg",
        &["test_x.tg"],
    );
    let reach = partition_reach(&injected);

    assert_eq!(reach.driver_reached, set(&["parser"]));
    assert!(reach.test_only.is_empty(), "{:?}", reach.test_only);
}

#[test]
fn a_module_no_entry_imports_is_reached_by_neither() {
    let injected = input(
        &["driver", "lexer::tests"],
        &[("main.tg", &["driver"])],
        "main.tg",
        &[],
    );
    let reach = partition_reach(&injected);

    assert_eq!(reach.unreached, set(&["lexer::tests"]));
    assert!(reach.test_only.is_empty(), "{:?}", reach.test_only);
}

#[test]
fn the_three_classes_partition_every_module_exactly_once() {
    let injected = input(
        &["a", "b", "c", "d"],
        &[("main.tg", &["a"]), ("test_x.tg", &["b", "c"])],
        "main.tg",
        &["test_x.tg"],
    );
    let reach = partition_reach(&injected);

    assert_eq!(reach.examined, 4);
    let total = reach.driver_reached.len() + reach.test_only.len() + reach.unreached.len();
    assert_eq!(total, 4);
    assert_eq!(reach.unreached, set(&["d"]));
}

#[test]
fn reach_follows_use_edges_transitively() {
    let injected = input(
        &["a", "b", "c"],
        &[("main.tg", &["a"]), ("a", &["b"]), ("b", &["c"])],
        "main.tg",
        &[],
    );
    assert_eq!(
        partition_reach(&injected).driver_reached,
        set(&["a", "b", "c"])
    );
}

#[test]
fn a_use_cycle_terminates() {
    let injected = input(
        &["a", "b"],
        &[("main.tg", &["a"]), ("a", &["b"]), ("b", &["a"])],
        "main.tg",
        &[],
    );
    assert_eq!(partition_reach(&injected).driver_reached, set(&["a", "b"]));
}

#[test]
fn an_entry_file_is_never_itself_classified() {
    // Entry files are keys in `uses` alongside modules; only `modules` is
    // partitioned, or every run would report its own roots as findings.
    let injected = input(
        &["a"],
        &[("main.tg", &["a"]), ("test_x.tg", &["a"])],
        "main.tg",
        &["test_x.tg"],
    );
    let reach = partition_reach(&injected);
    for class in [&reach.driver_reached, &reach.test_only, &reach.unreached] {
        assert!(!class.contains("main.tg"), "{class:?}");
        assert!(!class.contains("test_x.tg"), "{class:?}");
    }
}

#[test]
fn an_empty_graph_examines_nothing() {
    let reach = partition_reach(&input(&[], &[], "main.tg", &[]));
    assert_eq!(reach.examined, 0);
    assert!(reach.driver_reached.is_empty());
}
