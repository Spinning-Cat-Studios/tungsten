//! The inverse hint: executable, and nothing asserts it (ADR 19.8.26c review).
//!
//! The two halves are asserted apart because they fail apart — a name rule that
//! matched too much and a walk that found nothing would BOTH read as "untested".

use super::super::report::is_test_definition;
use super::*;

#[test]
fn a_test_prefix_is_read_on_the_last_path_segment_only() {
    assert!(is_test_definition("test_slice"));
    assert!(is_test_definition("driver::util::test_slice"));
    // The prefix must START the segment, not merely occur in it.
    assert!(!is_test_definition("contest_slice"));
    assert!(!is_test_definition("driver::test_util::helper"));
    assert!(!is_test_definition(""));
}

#[test]
fn a_test_that_reaches_the_target_is_named() {
    let globals = project(&[
        ("string_index_of_char", extern_wrapper("tg_println")),
        ("test_index", calls("string_index_of_char")),
    ]);
    let report = analyze_default(&globals, "string_index_of_char").unwrap();

    assert_eq!(report.reached_by_tests, vec!["test_index".to_string()]);
    assert!(!report.assertable_but_untested());
}

#[test]
fn an_executable_definition_no_test_reaches_is_the_finding() {
    let globals = project(&[
        ("string_index_of_char", extern_wrapper("tg_println")),
        ("test_something_else", Term::Unit),
    ]);
    let report = analyze_default(&globals, "string_index_of_char").unwrap();

    assert!(report.reached_by_tests.is_empty());
    assert!(report.assertable_but_untested());
}

#[test]
fn a_blocked_definition_is_never_reported_as_an_untested_opportunity() {
    // The flag means unused *cost-5* coverage. A definition that would go Stuck
    // can carry none, so calling it "untested" would send the reader to write an
    // assertion that never runs — the defect this command exists to warn about.
    let globals = project(&[("joins", extern_wrapper("tg_path_join"))]);
    let report = analyze_default(&globals, "joins").unwrap();

    assert_eq!(report.blocking().count(), 1);
    assert!(!report.assertable_but_untested());
}

#[test]
fn a_test_definition_is_not_an_untested_opportunity_against_itself() {
    let globals = project(&[("test_index", extern_wrapper("tg_println"))]);
    let report = analyze_default(&globals, "test_index").unwrap();

    // It reaches no OTHER test, so the walk is empty — but flagging a test as
    // untested is noise, and the root-is-a-test guard is what suppresses it.
    assert!(report.reached_by_tests.is_empty());
    assert!(!report.assertable_but_untested());
}

#[test]
fn an_indirect_reference_deliberately_does_not_count() {
    // The deliberate limitation, pinned so it cannot drift back to transitive
    // by accident. `test_parsed_module` transitively reaches most of the driver
    // on the real corpus, so counting indirect references made the flag fire
    // almost nowhere — measured, then rejected (see `TestReferences`).
    let globals = project(&[
        ("string_index_of_char", extern_wrapper("tg_println")),
        ("helper", calls("string_index_of_char")),
        ("test_index", calls("helper")),
    ]);
    let report = analyze_default(&globals, "string_index_of_char").unwrap();

    assert!(report.reached_by_tests.is_empty());
    assert!(report.assertable_but_untested());
}

#[test]
fn a_non_test_caller_never_counts_however_many_there_are() {
    // Production callers are not assertions. A definition with twenty callers
    // and no test is exactly the case this flag exists to surface.
    let globals = project(&[
        ("target", extern_wrapper("tg_println")),
        ("caller_one", calls("target")),
        ("caller_two", calls("target")),
    ]);
    let report = analyze_default(&globals, "target").unwrap();

    assert!(report.reached_by_tests.is_empty());
    assert!(report.assertable_but_untested());
}

#[test]
fn every_test_that_names_the_target_is_listed_alphabetically() {
    // The index iterates a `BTreeMap`, so order is a property of the data
    // structure — pinned here so a later change to a `HashMap` fails loudly
    // rather than making two runs of the same file diff.
    let globals = project(&[
        ("target", extern_wrapper("tg_println")),
        ("test_zulu", calls("target")),
        ("test_alpha", calls("target")),
    ]);
    let report = analyze_default(&globals, "target").unwrap();

    assert_eq!(
        report.reached_by_tests,
        vec!["test_alpha".to_string(), "test_zulu".to_string()]
    );
}
