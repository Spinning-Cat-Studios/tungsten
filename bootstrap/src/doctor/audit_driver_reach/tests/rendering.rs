//! The report's two emptiness findings, its classes, and `use`-path resolution.

use super::*;

/// ADR 3.9.26a AC3 — the empty input is a finding: zero modules examined must
/// not render like zero test-only modules.
#[test]
fn nothing_examined_does_not_render_like_nothing_test_only() {
    let empty = rendered(&[], &[], &["test_x.tg"]);
    let clean = rendered(&["a"], &[("main.tg", &["a"])], &["test_x.tg"]);

    assert!(empty.contains("no module was examined"), "{empty}");
    assert!(!clean.contains("no module was examined"), "{clean}");
    assert!(clean.contains("test-only: none"), "{clean}");
}

/// The report's second way of being empty, which means the opposite thing.
#[test]
fn no_test_entry_file_is_distinguishable_from_no_test_only_module() {
    let uncompared = rendered(&["a"], &[("main.tg", &["a"])], &[]);
    let compared = rendered(&["a"], &[("main.tg", &["a"])], &["test_x.tg"]);

    assert!(
        uncompared.contains("no test entry file was read"),
        "{uncompared}"
    );
    assert!(
        !compared.contains("no test entry file was read"),
        "{compared}"
    );
    assert!(compared.contains("test entries: test_x.tg"), "{compared}");
}

#[test]
fn the_reach_line_carries_both_denominators() {
    let out = rendered(
        &["a", "b"],
        &[("main.tg", &["a"])],
        &["test_x.tg", "test_y.tg"],
    );
    assert!(out.contains("2 module(s) examined"), "{out}");
    assert!(out.contains("3 entry file(s) (1 driver, 2 test)"), "{out}");
}

#[test]
fn an_unreadable_entry_file_is_named_rather_than_dropped() {
    let mut injected = input(&["a"], &[("main.tg", &["a"])], "main.tg", &["test_x.tg"]);
    injected.unreadable_entries = set(&["test_broken.tg"]);
    let out = render_reach(&partition_reach(&injected));

    assert!(out.contains("could not be parsed"), "{out}");
    assert!(out.contains("test_broken.tg"), "{out}");
}

#[test]
fn a_run_with_every_entry_readable_carries_no_parse_warning() {
    let out = rendered(&["a"], &[("main.tg", &["a"])], &["test_x.tg"]);
    assert!(!out.contains("could not be parsed"), "{out}");
}

#[test]
fn each_class_is_listed_with_its_members_and_a_count() {
    let out = rendered(
        &["driver", "codegen", "lexer::tests"],
        &[("main.tg", &["driver"]), ("test_c.tg", &["codegen"])],
        &["test_c.tg"],
    );
    assert!(out.contains("driver-reached: 1 module(s)"), "{out}");
    assert!(out.contains("test-only: 1 module(s)"), "{out}");
    assert!(out.contains("  codegen\n"), "{out}");
    assert!(out.contains("reached by neither: 1 module(s)"), "{out}");
    assert!(out.contains("  lexer::tests\n"), "{out}");
}

#[test]
fn the_report_says_a_mod_declaration_is_not_reach() {
    // The whole reason this tool is not `info module tree`.
    let out = rendered(&["a"], &[("main.tg", &["a"])], &["test_x.tg"]);
    assert!(out.contains("A `mod` declaration"), "{out}");
    assert!(out.contains("Reports, never gates"), "{out}");
}

// ---------------------------------------------------------------------------
// `use`-path resolution
// ---------------------------------------------------------------------------

#[test]
fn a_use_path_resolves_to_its_longest_known_module_prefix() {
    let known = set(&["codegen", "codegen::ir_types"]);
    let path = ["codegen", "ir_types", "llvm_type"].map(String::from);
    assert_eq!(
        graph::resolve_module(&path, &known),
        Some("codegen::ir_types".to_string())
    );
}

#[test]
fn a_whole_module_import_resolves_to_that_module_not_its_parent() {
    let known = set(&["codegen", "codegen::ir_types"]);
    let path = ["codegen", "ir_types"].map(String::from);
    assert_eq!(
        graph::resolve_module(&path, &known),
        Some("codegen::ir_types".to_string())
    );
}

#[test]
fn a_use_path_naming_nothing_known_resolves_to_nothing() {
    let known = set(&["codegen"]);
    let path = ["driver", "ffi"].map(String::from);
    assert_eq!(graph::resolve_module(&path, &known), None);
}
