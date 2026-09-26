//! Tests for the report text of `tungsten doctor check comparable`.
//!
//! Split from `tests.rs` for the file-size limit, along the seam that matters:
//! these assert what a READER is told, not what the analysis computes. Each
//! failing class must name its own cause, because the classes have different
//! fixes.
//!
//! Tests: bootstrap/src/doctor/checks/type_checks/check_comparable/render.rs

use super::render::{failure_label, render_summary};
use super::*;

fn failing(kind: ComparatorFailureKind) -> ComparabilityReport {
    ComparabilityReport {
        failure: Some(kind),
        closure_size: 0,
    }
}

// ── report rendering ────────────────────────────────────────────────────────

#[test]
fn clean_report_states_the_closure_size() {
    let report = ComparabilityReport {
        failure: None,
        closure_size: 4,
    };
    let out = render_report("Foo", &report);
    assert!(out.contains("✓ comparable"));
    assert!(out.contains('4'), "the reach number is the evidence: {out}");
}

#[test]
fn the_opaque_leaf_class_renders_its_own_section() {
    let out = render_report(
        "ElabDef",
        &failing(ComparatorFailureKind::OpaqueLeaf {
            path: "$.env: EvalEnv is opaque".to_string(),
        }),
    );
    assert!(
        out.contains("opaque leaf: $.env: EvalEnv is opaque"),
        "{out}"
    );
    assert!(
        out.contains("BY POLICY"),
        "must distinguish policy from defect: {out}"
    );
}

#[test]
fn the_incomplete_closure_class_names_the_dangling_symbol() {
    let out = render_report(
        "Alpha",
        &failing(ComparatorFailureKind::IncompleteClosure {
            dangling: "compare_AdtExpr_E".to_string(),
            cause: Some("$.type_params: List<TypeParam> is opaque".to_string()),
        }),
    );
    assert!(out.contains("compare_AdtExpr_E"), "{out}");
    assert!(
        out.contains("$.type_params"),
        "the symbol names the shape, not the reason — the cause must appear: {out}"
    );
}

/// …and when the walk cannot attribute the symbol, it must say so rather than
/// invent a cause. A renderer that always printed the same "usual cause" line
/// would pass the test above while being wrong here.
#[test]
fn an_unattributed_incomplete_closure_says_it_could_not_attribute() {
    let out = render_report(
        "Alpha",
        &failing(ComparatorFailureKind::IncompleteClosure {
            dangling: "compare_Named__Beta".to_string(),
            cause: None,
        }),
    );
    assert!(out.contains("compare_Named__Beta"), "{out}");
    assert!(out.contains("could not attribute"), "{out}");
}

#[test]
fn non_convergence_is_reported_and_points_at_the_unfold_factor() {
    let out = render_report(
        "Expr",
        &failing(ComparatorFailureKind::LimitExceeded { bound: 512 }),
    );
    assert!(out.contains("did not converge"));
    assert!(
        out.contains("info type size Expr"),
        "must name the follow-up tool with the type substituted: {out}"
    );
}

#[test]
fn the_empty_closure_class_renders_its_own_line() {
    let out = render_report("Weird", &failing(ComparatorFailureKind::EmptyClosure));
    assert!(out.contains("no comparator could be synthesized"), "{out}");
}

#[test]
fn a_missing_synthesizer_is_reported_as_a_wiring_fault_not_a_type_fact() {
    let out = render_report("Nat", &failing(ComparatorFailureKind::NoSynthesizer));
    assert!(
        out.contains("wiring fault"),
        "an absent callback says nothing about the type: {out}"
    );
}

/// Every failing report ends with the consequence, because "this type is not
/// comparable" without "and here is what a run will do" is the half that gets
/// ignored.
#[test]
fn every_failing_class_renders_its_own_text_and_the_consequence() {
    let kinds = [
        ComparatorFailureKind::OpaqueLeaf {
            path: "$.f: Arrow is opaque".to_string(),
        },
        ComparatorFailureKind::EmptyClosure,
        ComparatorFailureKind::IncompleteClosure {
            dangling: "compare_X".to_string(),
            cause: None,
        },
        ComparatorFailureKind::LimitExceeded { bound: 512 },
        ComparatorFailureKind::NoSynthesizer,
    ];
    let mut bodies = Vec::new();
    for kind in kinds {
        let out = render_report("T", &failing(kind.clone()));
        assert!(
            out.contains("fails the run"),
            "missing the consequence for {kind:?}"
        );
        bodies.push(out);
    }
    // Asserting only the shared consequence line lets any single renderer be
    // deleted without a test noticing (measured on the pre-1.8.26b renderer:
    // `replace render_opaque_leaf with ()` survived exactly that way).
    for (i, a) in bodies.iter().enumerate() {
        for b in bodies.iter().skip(i + 1) {
            assert_ne!(a, b, "two failure classes render identically");
        }
    }
}

#[test]
fn is_comparable_is_exactly_the_absence_of_a_failure() {
    assert!(ComparabilityReport {
        failure: None,
        closure_size: 1
    }
    .is_comparable());
    assert!(!failing(ComparatorFailureKind::EmptyClosure).is_comparable());
    assert_eq!(
        failing(ComparatorFailureKind::EmptyClosure).exit(),
        ExitCode::from(1)
    );
}

// ── the `--all` summary ─────────────────────────────────────────────────────

#[test]
fn the_summary_lists_every_type_and_details_only_the_failures() {
    let reports = vec![
        (
            "Alpha",
            ComparabilityReport {
                failure: None,
                closure_size: 3,
            },
        ),
        (
            "Beta",
            failing(ComparatorFailureKind::OpaqueLeaf {
                path: "$.f: Arrow is opaque".to_string(),
            }),
        ),
    ];
    let out = render_summary(&reports);

    // Every type appears in the scan table…
    assert!(out.contains("Alpha"), "{out}");
    assert!(out.contains("Beta"), "{out}");
    // …but only the failure gets its explanation, or a clean corpus of 200
    // types would print 200 sections.
    assert!(out.contains("$.f: Arrow is opaque"), "{out}");
    assert!(
        !out.contains("── Alpha ──"),
        "a comparable type needs no detail section: {out}"
    );
    assert!(out.contains("1 of 2"), "the tally is the headline: {out}");
}

#[test]
fn an_all_comparable_summary_says_so_without_a_failure_section() {
    let reports = vec![
        (
            "Alpha",
            ComparabilityReport {
                failure: None,
                closure_size: 3,
            },
        ),
        (
            "Beta",
            ComparabilityReport {
                failure: None,
                closure_size: 1,
            },
        ),
    ];
    let out = render_summary(&reports);
    assert!(out.contains("all 2 type(s) comparable"), "{out}");
    assert!(
        !out.contains("cannot be compared"),
        "no failure section on a clean corpus: {out}"
    );
}

/// The scan column must distinguish the classes — a summary that labelled every
/// failure the same way would force a reader into the detail section for each.
#[test]
fn each_failure_class_gets_a_distinct_summary_label() {
    let kinds = [
        ComparatorFailureKind::OpaqueLeaf { path: "$".into() },
        ComparatorFailureKind::EmptyClosure,
        ComparatorFailureKind::IncompleteClosure {
            dangling: "compare_X".into(),
            cause: None,
        },
        ComparatorFailureKind::LimitExceeded { bound: 512 },
        ComparatorFailureKind::NoSynthesizer,
    ];
    let labels: Vec<&str> = kinds.iter().map(failure_label).collect();
    for (i, a) in labels.iter().enumerate() {
        assert!(!a.is_empty());
        for b in labels.iter().skip(i + 1) {
            assert_ne!(a, b, "two classes share a summary label");
        }
    }
}
