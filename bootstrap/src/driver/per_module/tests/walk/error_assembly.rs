//! Error assembly: canonical ordering, pre-walk (Signature Collection)
//! placement, and the same-fault fold (ADR 14.8.26g D1/D4/D2a). Split from
//! `walk_accumulation.rs` when D4's tests took it over the file-size limit.

use crate::driver::per_module::accumulator::ModuleTreeAccumulator;
use crate::elaborate::{ElabError, ElabErrorKind};
use crate::span::Span;

// =========================================================================
// take_module_errors_ordered — canonical ordering (ADR 14.8.26g D1)
// =========================================================================

fn error_named(msg: &str) -> ElabError {
    ElabError::new(Span::new(0, 0), ElabErrorKind::Other(msg.to_string()))
}

fn error_named_at(msg: &str, offset: u32) -> ElabError {
    ElabError::new(
        Span::new(offset, offset + 1),
        ElabErrorKind::Other(msg.to_string()),
    )
}

fn path(segments: &[&str]) -> Vec<String> {
    segments.iter().map(|s| s.to_string()).collect()
}

/// The `Other(msg)` payloads of an error list, in order.
fn other_messages(errors: &[ElabError]) -> Vec<&str> {
    errors
        .iter()
        .map(|e| match &e.kind {
            ElabErrorKind::Other(msg) => msg.as_str(),
            other => panic!("fixture errors are all Other(_): {other:?}"),
        })
        .collect()
}

/// Groups pushed in a scrambled (parallel-merge) order come out in the
/// canonical (serial-walk) order, so diagnostics cannot depend on
/// `thread_count`.
#[test]
fn module_error_groups_are_reported_in_canonical_order() {
    let canonical = [path(&["a"]), path(&["b"]), path(&[])];
    let mut acc = ModuleTreeAccumulator::new();
    acc.module_errors
        .push((path(&[]), vec![error_named("root")]));
    acc.module_errors
        .push((path(&["b"]), vec![error_named("b")]));
    acc.module_errors
        .push((path(&["a"]), vec![error_named("a")]));

    let ordered = acc.take_accumulated_errors_ordered(&canonical);
    assert_eq!(other_messages(&ordered), vec!["a", "b", "root"]);
    assert!(
        !acc.has_accumulated_errors(),
        "take_accumulated_errors_ordered must drain the groups"
    );
}

/// Pre-walk (Signature Collection, D4) errors report before every module
/// group — the root cause reads first.
#[test]
fn pre_walk_errors_report_before_module_groups() {
    // Distinct spans: an identical (file, span, code) would be the same
    // fault twice, which the assembly-dedup test below pins to fold.
    let canonical = [path(&["a"])];
    let mut acc = ModuleTreeAccumulator::new();
    acc.module_errors
        .push((path(&["a"]), vec![error_named_at("module", 10)]));
    acc.pre_walk_errors
        .push(error_named_at("signature-collection", 20));

    let ordered = acc.take_accumulated_errors_ordered(&canonical);
    assert_eq!(
        other_messages(&ordered),
        vec!["signature-collection", "module"]
    );
}

/// The global pass elaborates the same items the module passes do, so its
/// copy of a fault a module also reported is dropped at assembly — pre-dedup,
/// keeping the raw diagnostic count honest (D4 + D2a).
#[test]
fn a_pre_walk_copy_of_a_module_error_is_dropped_at_assembly() {
    let canonical = [path(&["a"])];
    let mut acc = ModuleTreeAccumulator::new();
    acc.module_errors
        .push((path(&["a"]), vec![error_named_at("same-fault", 10)]));
    acc.pre_walk_errors.push(error_named_at("same-fault", 10));

    let ordered = acc.take_accumulated_errors_ordered(&canonical);
    assert_eq!(
        other_messages(&ordered),
        vec!["same-fault"],
        "the same (file, span, code) must be reported once, pre-dedup"
    );
}

/// A failed pre-walk pass alone must fail the run (D4), even when every
/// module then elaborates cleanly.
#[test]
fn pre_walk_errors_alone_fail_the_run() {
    let mut acc = ModuleTreeAccumulator::new();
    assert!(!acc.has_accumulated_errors());
    acc.pre_walk_errors
        .push(error_named("signature-collection"));
    assert!(acc.has_accumulated_errors());
}

/// A group whose path is missing from the canonical order is reported last,
/// not lost.
#[test]
fn an_unknown_module_path_sorts_last_not_lost() {
    let canonical = [path(&["a"])];
    let mut acc = ModuleTreeAccumulator::new();
    acc.module_errors
        .push((path(&["ghost"]), vec![error_named("ghost")]));
    acc.module_errors
        .push((path(&["a"]), vec![error_named("a")]));

    let ordered = acc.take_accumulated_errors_ordered(&canonical);
    assert_eq!(other_messages(&ordered), vec!["a", "ghost"]);
}

/// The warning text carries the count, the first error, and the remediation
/// hint — and expands per-error under verbose. Built as a string precisely so
/// this is assertable rather than write-only stderr.
#[test]
fn the_signature_collection_warning_names_count_and_hint() {
    let errors = vec![
        error_named_at("first-fault", 5),
        error_named_at("second", 9),
    ];
    let warning = crate::driver::per_module::phases::signature_collection_warning(&errors, false);
    assert!(warning.contains("failed with 2 errors"), "{warning}");
    assert!(warning.contains("first-fault"), "{warning}");
    assert!(
        warning.contains("doctor check module signature-collection"),
        "{warning}"
    );
    assert!(!warning.contains("second"), "non-verbose must not expand");
    let verbose = crate::driver::per_module::phases::signature_collection_warning(&errors, true);
    assert!(verbose.contains("second"), "verbose must expand: {verbose}");
}
