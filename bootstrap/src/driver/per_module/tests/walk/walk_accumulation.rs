//! The module walk accumulates instead of short-circuiting (ADR 14.8.26g D1).
//!
//! The e2e test here is the accumulation's must-fail twin in spirit: under the
//! old walk, the first failing sibling ended the level, so only one file's
//! errors could ever appear. Both files reporting is what distinguishes the
//! accumulating walk from the short-circuiting one.

use std::fs;

use super::{elaborate_tree_errors, elaborate_tree_errors_with_threads, failing_files};
use crate::driver::pipeline;
use crate::driver::prepare_project;
use crate::elaborate::ElabErrorKind;

#[test]
fn two_sibling_faults_are_both_reported() {
    let errors = elaborate_tree_errors(&[
        ("main.tg", "mod alpha;\nmod beta;\n\nfn main() -> Nat { 0 }"),
        ("alpha.tg", "pub fn alpha_broken() -> Nat { \"not a nat\" }"),
        ("beta.tg", "pub fn beta_broken() -> Bool { 42 }"),
    ]);
    let files = failing_files(&errors);
    assert!(
        files.contains(&"alpha.tg".to_string()) && files.contains(&"beta.tg".to_string()),
        "both independently-broken siblings must report; got errors in {files:?}",
    );
}

#[test]
fn a_clean_tree_still_elaborates_clean() {
    let errors = elaborate_tree_errors(&[
        ("main.tg", "mod alpha;\n\nfn main() -> Nat { helper() }"),
        ("alpha.tg", "pub fn helper() -> Nat { 42 }"),
    ]);
    assert!(
        errors.is_empty(),
        "clean fixture must stay clean: {errors:?}"
    );
}

/// A fault in a module the old walk would have stopped at must not hide a
/// fault in a module elaborated *after* it (post-order: children first, so a
/// broken child used to hide a broken parent).
#[test]
fn a_broken_child_does_not_hide_a_broken_parent() {
    let errors = elaborate_tree_errors(&[
        (
            "main.tg",
            "mod alpha;\n\nfn main_broken() -> Nat { \"also not a nat\" }",
        ),
        ("alpha.tg", "pub fn alpha_broken() -> Nat { \"not a nat\" }"),
    ]);
    let files = failing_files(&errors);
    assert!(
        files.contains(&"alpha.tg".to_string()) && files.contains(&"main.tg".to_string()),
        "the parent's fault must survive the child's; got errors in {files:?}",
    );
}

/// The small-fixture analog of the V1 shape, at the P3 endpoint: the
/// collection-pass deferral (D2) runs Pass 2 against the poisoned signature
/// (D3), so the seeded E0002 is reported as an error at its own span — on
/// `main` it was demoted to a Signature Collection warning line — while the
/// dependent module's call site compares against `Type::Error` and stays
/// QUIET. During the unconditional P1b→P3 window this fixture reported a
/// cascade E0001 in `consumer.tg`; the producers are what suppress it, so
/// cascade reappearing here means a producer regressed.
#[test]
fn a_seeded_signature_fault_reports_at_its_span_and_nowhere_else() {
    let errors = elaborate_tree_errors(&[
        (
            "main.tg",
            "mod lib;\nmod consumer;\n\nfn main() -> Nat { 0 }",
        ),
        ("lib.tg", "pub fn helper() -> NoSuchType { 0 }"),
        (
            "consumer.tg",
            "use lib::{helper};\npub fn use_it() -> Nat { helper() }",
        ),
    ]);
    let seeded_reported = errors.iter().any(|e| {
        matches!(&e.kind, ElabErrorKind::UndefinedType(name) if name == "NoSuchType")
            && e.file_path.as_ref().is_some_and(|p| p.ends_with("lib.tg"))
    });
    assert!(
        seeded_reported,
        "the seeded fault must be reported as an error at its own span, not \
         only as a Signature Collection warning: {errors:?}",
    );
    let files = failing_files(&errors);
    assert!(
        !files.contains(&"consumer.tg".to_string()),
        "the dependent's call site elaborates against poison and must not \
         cascade; got errors in {files:?}",
    );
}

/// A failed global Signature Collection sets the annotation flag and enters
/// the run's pre-walk error list (D4); a clean one touches neither. This is
/// `run_signature_collection`'s whole observable contract — the warning it
/// prints is tested as text separately.
#[test]
fn a_failed_signature_collection_marks_the_accumulator() {
    use crate::driver::per_module::accumulator::ModuleTreeAccumulator;
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("main.tg"),
        "pub fn helper() -> NoSuchType { 0 }",
    )
    .unwrap();
    let main_path = dir.path().join("main.tg");
    let prepared = prepare_project(&main_path, false, None).expect("fixture must parse");
    let build = pipeline::BuildCtx {
        cache: None,
        module_info: prepared.module_info,
        source_map: prepared.source_map,
    };
    let mut acc = ModuleTreeAccumulator::new();
    crate::driver::per_module::phases::run_signature_collection(
        &prepared.module_tree,
        &build,
        &mut acc,
        false,
    );
    assert!(!acc.signature_collection_ok, "the failure must be marked");
    assert!(
        !acc.pre_walk_errors.is_empty(),
        "the dropped errors must enter the run's list (D4)"
    );
}

/// The canonical order is the serial traversal: children (dependency-sorted)
/// before self, root last — computed from the tree, not recorded during the
/// walk, so the parallel walker can be re-ordered against it.
#[test]
fn canonical_walk_order_is_children_then_self() {
    let mut root = crate::driver::per_module::tests::make_parsed_module(vec![]);
    root.submodules
        .push(crate::driver::per_module::tests::make_parsed_module(vec![]));
    root.submodules
        .push(crate::driver::per_module::tests::make_parsed_module(vec![]));
    let mut order = Vec::new();
    crate::driver::per_module::walk::canonical_walk_order(&root, &[], &mut order);
    assert_eq!(
        order.len(),
        3,
        "every module appears once, root included: {order:?}"
    );
    assert_eq!(
        order.last(),
        Some(&Vec::new()),
        "the root (empty path) comes after its children"
    );
}

/// D6's module bail-out: at a display budget of 2, a three-fault project
/// stops after two failing modules; at the measurement setting (0) it never
/// bails. The budget is read from the thread-local `--max-errors` setting.
#[test]
fn the_module_bail_out_stops_at_the_display_budget() {
    let files: [(&str, &str); 4] = [
        (
            "main.tg",
            "mod alpha;\nmod beta;\nmod gamma;\n\nfn main() -> Nat { 0 }",
        ),
        ("alpha.tg", "pub fn alpha_broken() -> Nat { \"a\" }"),
        ("beta.tg", "pub fn beta_broken() -> Bool { 7 }"),
        ("gamma.tg", "pub fn gamma_broken() -> String { 9 }"),
    ];
    crate::driver::diagnostics::set_max_errors(2);
    let bailed = elaborate_tree_errors(&files);
    crate::driver::diagnostics::set_max_errors(0);
    let exhaustive = elaborate_tree_errors(&files);
    crate::driver::diagnostics::set_max_errors(20);
    assert_eq!(
        failing_files(&bailed).len(),
        2,
        "budget 2 must stop after two failing modules: {bailed:?}"
    );
    assert_eq!(
        failing_files(&exhaustive).len(),
        3,
        "budget 0 must never bail: {exhaustive:?}"
    );
}

/// The parallel walker reports the same faults the serial one does
/// (ADR 14.8.26g D1): worker accumulators carry their errors to the level
/// merge, and the canonical re-ordering makes the report identical. This is
/// the unit-level arm of the determinism criterion; the byte-identical e2e
/// comparison ran at close-out (10 runs, thread_count 1 vs 8).
#[test]
fn the_parallel_walker_reports_the_same_faults_as_the_serial_one() {
    let files: [(&str, &str); 4] = [
        (
            "main.tg",
            "mod alpha;\nmod beta;\nmod gamma;\n\nfn main() -> Nat { 0 }",
        ),
        ("alpha.tg", "pub fn alpha_broken() -> Nat { \"a\" }"),
        ("beta.tg", "pub fn beta_broken() -> Bool { 7 }"),
        ("gamma.tg", "pub fn gamma_broken() -> String { 9 }"),
    ];
    let serial = elaborate_tree_errors_with_threads(&files, 1);
    let parallel = elaborate_tree_errors_with_threads(&files, 4);
    assert_eq!(serial.len(), parallel.len(), "same fault count");
    assert_eq!(
        failing_files(&serial),
        failing_files(&parallel),
        "same files, same order"
    );
}

/// The V4 shape: an unresolved `use` is an unpoisoned error, so the failing
/// module's own collection short-circuits — but the GLOBAL Signature
/// Collection pass defers unconditionally, keeping its partial exports, so
/// unrelated modules still see the full signature environment and stay
/// quiet. The regression this pins (measured during P3): the global pass
/// short-circuiting threw away every collected signature and one bad import
/// became 894 reported errors across the corpus.
#[test]
fn an_unresolved_use_stays_contained() {
    let errors = elaborate_tree_errors(&[
        (
            "main.tg",
            "mod lib;\nmod broken_import;\nmod consumer;\n\nfn main() -> Nat { 0 }",
        ),
        ("lib.tg", "pub fn helper() -> Nat { 1 }"),
        (
            "broken_import.tg",
            "use nonexistent_module::{missing_thing};\npub fn unrelated() -> Nat { 2 }",
        ),
        (
            "consumer.tg",
            "use lib::{helper};\npub fn use_it() -> Nat { helper() }",
        ),
    ]);
    let files = failing_files(&errors);
    assert!(
        !files.contains(&"consumer.tg".to_string()) && !files.contains(&"lib.tg".to_string()),
        "unrelated modules must stay quiet; got errors in {files:?}",
    );
    assert!(
        !errors.is_empty() && errors.len() <= 3,
        "one bad import must stay contained: {} errors",
        errors.len()
    );
}

// =========================================================================
// The walk's two extracted predicates (ADR 14.8.26g retrospective)
// =========================================================================

use crate::driver::per_module::walk::{
    module_failure_budget_reached, should_elaborate_children_in_parallel,
};

/// The bail-out budget: 0 means unlimited (the measurement setting), and the
/// comparison is "reached", not "exceeded" — a budget of 2 stops AT two
/// failing modules, because a third could not be displayed anyway.
#[test]
fn the_failure_budget_is_reached_not_exceeded() {
    assert!(!module_failure_budget_reached(0, 0), "0 = unlimited");
    assert!(!module_failure_budget_reached(0, 99), "0 stays unlimited");
    assert!(!module_failure_budget_reached(2, 0));
    assert!(
        !module_failure_budget_reached(2, 1),
        "below budget walks on"
    );
    assert!(module_failure_budget_reached(2, 2), "at budget stops");
    assert!(module_failure_budget_reached(2, 3), "past budget stops");
}

/// Parallel scheduling needs BOTH a thread budget and something to schedule;
/// a lone submodule goes serial however many threads are configured.
#[test]
fn parallel_scheduling_needs_threads_and_siblings() {
    assert!(should_elaborate_children_in_parallel(2, 2));
    assert!(
        !should_elaborate_children_in_parallel(1, 2),
        "serial config"
    );
    assert!(!should_elaborate_children_in_parallel(2, 1), "one child");
    assert!(!should_elaborate_children_in_parallel(1, 1));
    assert!(!should_elaborate_children_in_parallel(2, 0), "no children");
}
