//! The cache commit gate (ADR 14.8.26g D5) and the run-level codegen
//! refusal (D8). Split from `walk_accumulation.rs` when the walk's own tests
//! took that file over the size threshold.

use std::fs;

use super::{elab_cache_entry_count, elaborate_tree_with_cache};

/// A run that had a failure commits no cache entry — including the sibling
/// that elaborated cleanly. The accumulating walk now *reaches* that sibling,
/// which is exactly what makes this gate necessary: before D1 the failure
/// stopped the walk, so nothing downstream was ever cached by accident.
#[test]
fn a_failed_run_commits_no_cache_entries() {
    let dir = tempfile::tempdir().unwrap();
    let result = elaborate_tree_with_cache(
        dir.path(),
        &[
            ("main.tg", "mod good;\nmod bad;\n\nfn main() -> Nat { 0 }"),
            ("good.tg", "pub fn fine() -> Nat { 1 }"),
            ("bad.tg", "pub fn broken() -> Nat { \"nope\" }"),
        ],
    );
    assert!(result.is_err(), "the seeded fault must fail the run");
    assert_eq!(
        elab_cache_entry_count(dir.path()),
        0,
        "a failing run must leave the cache exactly as it found it"
    );
}

/// The positive control that keeps the gate test from passing vacuously: the
/// same shape without the fault does commit entries. Also the "subsequent
/// clean run" half of the D5 criterion — a clean run after a failed one
/// starts from an empty cache, identical to a cold-cache clean run.
#[test]
fn a_clean_run_after_a_failed_one_matches_cold_cache() {
    let dir = tempfile::tempdir().unwrap();
    let failed = elaborate_tree_with_cache(
        dir.path(),
        &[
            ("main.tg", "mod good;\n\nfn main() -> Nat { broken() }"),
            ("good.tg", "pub fn broken() -> Nat { \"nope\" }"),
        ],
    );
    assert!(failed.is_err());
    assert_eq!(elab_cache_entry_count(dir.path()), 0);

    // Fix the fault; the clean run must now commit, from a cache state
    // identical to one the failed run never touched.
    let fixed = elaborate_tree_with_cache(
        dir.path(),
        &[
            ("main.tg", "mod good;\n\nfn main() -> Nat { broken() }"),
            ("good.tg", "pub fn broken() -> Nat { 1 }"),
        ],
    );
    assert!(fixed.is_ok(), "the fixed fixture must elaborate: {fixed:?}");
    assert!(
        elab_cache_entry_count(dir.path()) > 0,
        "a clean run must commit its staged entries"
    );
}

/// D8: a run that accumulated any error refuses to enter codegen — the
/// `compile`/`check` entry point returns `Err` before any codegen unit is
/// built.
#[test]
fn a_failing_run_refuses_codegen_entry() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("main.tg"),
        "mod good;\nmod bad;\n\nfn main() -> Nat { 0 }",
    )
    .unwrap();
    fs::write(dir.path().join("good.tg"), "pub fn fine() -> Nat { 1 }").unwrap();
    fs::write(
        dir.path().join("bad.tg"),
        "pub fn broken() -> Nat { \"nope\" }",
    )
    .unwrap();
    let result = crate::driver::elaborate_project(&dir.path().join("main.tg"), false, 0, None);
    assert!(
        result.is_err(),
        "a run that accumulated an error must not reach codegen"
    );
}
