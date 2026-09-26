//! Tests for the audit's **summary** surface: the per-arm candidate/tracked
//! counters, their aggregation across modules, vacuous-pass detection
//! (ADR 2.7.26b T5a), and the finding-kind explanations the report prints.

use super::*;
use std::path::Path;

/// A canonical `$direct` shim (sret + one indirect buffer), so a fixture can
/// carry shim-arm counts as well as callee-arm ones.
const SHIM_MODULE: &str = r#"
define { i64, i64 } @"ok$direct"(ptr %0, { { i64, i64 }, i64 } %1) {
entry:
  %sret_buf = alloca { i64, i64 }, align 8
  %indirect_buf.0 = alloca { { i64, i64 }, i64 }, align 8
  store { { i64, i64 }, i64 } %1, ptr %indirect_buf.0, align 8
  call void @"ok$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %sret_buf, ptr noalias nonnull align 8 dereferenceable(24) %indirect_buf.0, ptr %0, i64 0)
  %sret_load = load { i64, i64 }, ptr %sret_buf, align 8
  ret { i64, i64 } %sret_load
}
"#;

// ── Vacuous-pass detection (ADR 2.7.26b T5a) ─────────────────────────────────

/// Attributed corpus in the real (post-1.7.26e P6) format: candidates > 0 AND
/// tracked > 0 — a genuine, non-vacuous pass.
const GOOD_ATTRIBUTED: &str = r#"
define void @"ok$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr nonnull align 8 dereferenceable(24) %1, ptr %2, i64 %3) {
entry:
  store { i64, i64 } zeroinitializer, ptr %0, align 8
  musttail call void @"ok$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr nonnull align 8 dereferenceable(24) %1, ptr null, i64 %3)
  ret void
}
"#;

#[test]
fn attributed_corpus_is_non_vacuous_pass() {
    let s = audit_ir_summary(GOOD_ATTRIBUTED);
    assert!(
        s.findings.is_empty(),
        "clean attributed IR must pass: {:?}",
        s.findings
    );
    assert_eq!(
        s.callee.candidates, 1,
        "sret/dereferenceable header is a candidate"
    );
    assert_eq!(
        s.callee.tracked, 2,
        "both buffers forwarded across the musttail: {}",
        s.callee.tracked
    );
    assert!(!s.is_vacuous());
}

/// Vacuity signal: a candidate `$direct_mt` (buffer-carrying header) where the
/// audit tracks NOTHING — e.g. parser/format drift making the forwarded args
/// unrecognizable. Candidates > 0, tracked == 0 → strict must fail.
#[test]
fn vacuous_corpus_has_candidates_but_no_tracked_buffers() {
    // No self-musttail the parser recognizes → zero tracked buffers.
    let drifted = r#"
define void @"drift$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr %1, i64 %2) {
entry:
  store { i64, i64 } zeroinitializer, ptr %0, align 8
  ret void
}
"#;
    let s = audit_ir_summary(drifted);
    assert_eq!(s.callee.candidates, 1);
    assert_eq!(s.callee.tracked, 0, "nothing forwarded → nothing tracked");
    assert!(
        s.findings.is_empty(),
        "vacuity is not a violation, it is a warning/strict failure"
    );
    assert!(s.is_vacuous(), "strict mode must fail this corpus");
}

/// A scalar-only `$direct_mt` (no sret/indirect buffer param) legitimately
/// forwards zero buffers: candidates == 0 → NOT vacuous, passes even strict.
#[test]
fn scalar_only_direct_mt_is_not_a_candidate() {
    let scalar_only = r#"
define i64 @"count$direct_mt"(ptr %0, i64 %1) {
entry:
  %next = add i64 %1, 1
  %r = musttail call i64 @"count$direct_mt"(ptr null, i64 %next)
  ret i64 %r
}
"#;
    let s = audit_ir_summary(scalar_only);
    assert_eq!(
        s.callee.candidates, 0,
        "scalar-only header is not a candidate"
    );
    assert_eq!(s.callee.tracked, 0);
    assert!(s.findings.is_empty());
    assert!(!s.is_vacuous());
}

/// Both counts flow through the multi-function summary (the machine-readable
/// output aggregates per-module summaries).
#[test]
fn summary_counts_aggregate_across_functions() {
    let combined = format!("{GOOD_ATTRIBUTED}\n{GOOD_ATTRIBUTED}");
    let s = audit_ir_summary(&combined);
    assert_eq!(s.callee.candidates, 2, "both functions are candidates");
    assert_eq!(
        s.callee.tracked, 4,
        "two buffers each: {}",
        s.callee.tracked
    );
}

#[test]
fn attributed_call_args_are_still_tracked() {
    // Regression (ADR 1.7.26e P6): with canonical slot attributes the musttail
    // args read `ptr nonnull align 8 dereferenceable(24) %0` — the audit must
    // still recognize %0 as the forwarded buffer (and catch its escapes).
    let bad = r#"
define i64 @"leak6$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr nonnull align 8 dereferenceable(24) %1, ptr %2, i64 %3) {
entry:
  store ptr %1, ptr @global, align 8
  musttail call void @"leak6$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr nonnull align 8 dereferenceable(24) %1, ptr null, i64 %3)
  ret void
}
"#;
    let f = audit_ir(bad);
    assert!(
        f.iter().any(|x| x.kind == FindingKind::PointerEscape),
        "attributed buffer args must still be tracked: {f:?}"
    );
}

/// Each finding kind renders a distinct, non-empty explanation naming the
/// invariant it protects — the printed text is the audit's whole user surface.
#[test]
fn finding_kinds_render_distinct_explanations() {
    let kinds = [
        (FindingKind::ForwardedAlloca, "R10"),
        (FindingKind::PointerEscape, "R4"),
        (FindingKind::ShimBufferEscape, "I4"),
        (FindingKind::TailEdgeAliasedForward, "I5"),
    ];
    let mut seen: Vec<&str> = Vec::new();
    for (kind, invariant) in kinds {
        let text = kind.human();
        assert!(
            text.contains(invariant),
            "{kind:?} must name {invariant}: {text}"
        );
        assert!(!seen.contains(&text), "duplicate explanation: {text}");
        seen.push(text);
    }
}

/// A module whose buffer escapes, so a fixture can carry a finding.
const LEAKY_MODULE: &str = r#"
define i64 @"leak8$direct_mt"(ptr nonnull align 8 dereferenceable(24) %0, ptr %1, i64 %2) {
entry:
  store ptr %0, ptr @global, align 8
  %r = musttail call i64 @"leak8$direct_mt"(ptr nonnull align 8 dereferenceable(24) %0, ptr null, i64 %2)
  ret i64 %r
}
"#;

/// Per-module counts fold into a running total by **addition**, both arms.
///
/// Asserting on whole `ReachCounts` values (rather than field by field) is the
/// point of the type: a fold that updated `candidates` but forgot `tracked`
/// fails here without anyone having to remember to check the second field.
#[test]
fn add_counts_sums_every_arm() {
    let mut totals = AuditSummary::default();

    // A module carrying BOTH arms' counts, so neither arm's fold goes untested.
    let both_arms = format!("{GOOD_ATTRIBUTED}\n{SHIM_MODULE}");
    totals.add_counts(&audit_ir_summary(&both_arms));
    let one_of_each = ReachCounts {
        candidates: 1,
        tracked: 2,
    };
    assert_eq!(totals.callee, one_of_each);
    assert_eq!(totals.shim, one_of_each);

    totals.add_counts(&audit_ir_summary(&both_arms));
    let two_of_each = ReachCounts {
        candidates: 2,
        tracked: 4,
    };
    assert_eq!(totals.callee, two_of_each, "callee counts accumulate");
    assert_eq!(totals.shim, two_of_each, "shim counts accumulate");

    totals.add_counts(&audit_ir_summary(LEAKY_MODULE));
    assert_eq!(
        totals.callee,
        ReachCounts {
            candidates: 3,
            tracked: 5
        }
    );
    assert_eq!(totals.shim, two_of_each, "a callee-only module leaves shim");
}

/// Findings come back tagged with the `.ll` file they were found in — how the
/// directory scan attributes a violation to its source.
#[test]
fn locate_findings_tags_each_finding_with_its_file() {
    assert!(
        locate_findings(
            audit_ir_summary(GOOD_ATTRIBUTED).findings,
            Path::new("a.ll")
        )
        .is_empty(),
        "a clean module contributes no findings"
    );

    let located = locate_findings(audit_ir_summary(LEAKY_MODULE).findings, Path::new("c.ll"));
    assert_eq!(located.len(), 1);
    assert_eq!(located[0].file, "c.ll", "finding carries its source file");
    assert_eq!(located[0].finding.kind, FindingKind::PointerEscape);
}

// ── The directory scan's verdict (ADR 17.7.26e close-out) ───────────────────

/// Write `.ll` modules into a fresh directory the scan can walk.
fn corpus_dir(modules: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (i, module) in modules.iter().enumerate() {
        std::fs::write(dir.path().join(format!("m{i}.ll")), module).expect("write .ll");
    }
    dir
}

/// The verdict is a value, not just an exit code — `ExitCode` has no
/// `PartialEq`, so a test asserting only on it asserts nothing.
#[test]
fn clean_corpus_is_clean_under_both_strictness_settings() {
    let dir = corpus_dir(&[GOOD_ATTRIBUTED, SHIM_MODULE]);
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Clean,
            "strict={strict}"
        );
    }
}

/// Vacuity is a warning on its own and a failure under `--strict` — the T5a
/// contract, asserted on the real directory scan rather than on counts alone.
#[test]
fn vacuous_corpus_fails_only_under_strict() {
    // Candidate header, no self-musttail → candidates > 0, tracked == 0.
    let drifted = r#"
define void @"drift$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr %1, i64 %2) {
entry:
  store { i64, i64 } zeroinitializer, ptr %0, align 8
  ret void
}
"#;
    let dir = corpus_dir(&[drifted]);
    assert_eq!(audit_directory(dir.path(), false), AuditVerdict::Clean);
    assert_eq!(
        audit_directory(dir.path(), true),
        AuditVerdict::VacuousUnderStrict
    );
}

/// Violations outrank vacuity: a corpus with real findings reports them even
/// when an arm also proved nothing, so the actionable defect is never masked.
#[test]
fn violations_are_reported_ahead_of_vacuity() {
    let dir = corpus_dir(&[LEAKY_MODULE]);
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Violations(1),
            "strict={strict}"
        );
    }
}

/// A path that is not a directory is distinguishable from a clean scan.
#[test]
fn non_directory_path_is_its_own_outcome() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("not-a-dir.ll");
    std::fs::write(&file, GOOD_ATTRIBUTED).expect("write");
    assert_eq!(audit_directory(&file, false), AuditVerdict::NotADirectory);
    assert_eq!(
        audit_directory(&dir.path().join("missing"), false),
        AuditVerdict::NotADirectory
    );
}

/// Only `Clean` succeeds; every other verdict is a non-zero exit — and the
/// command itself carries that mapping through to its process status.
///
/// The end-to-end half matters more than it looks: a gate whose entry point
/// always returns success is a gate that proves nothing, which is the exact
/// failure this audit exists to catch. `ExitCode` has no `PartialEq`, so the
/// comparison goes through `Debug`.
#[test]
fn only_a_clean_verdict_exits_successfully() {
    let success = format!("{:?}", AuditVerdict::Clean.exit());
    let failures = [
        AuditVerdict::NotADirectory,
        AuditVerdict::Violations(1),
        AuditVerdict::VacuousUnderStrict,
    ];
    for outcome in &failures {
        assert_ne!(
            format!("{:?}", outcome.exit()),
            success,
            "{outcome:?} must not exit like Clean"
        );
    }

    let clean = corpus_dir(&[GOOD_ATTRIBUTED]);
    let leaky = corpus_dir(&[LEAKY_MODULE]);
    assert_eq!(
        format!("{:?}", cmd_check_indirect_buffers(clean.path(), true)),
        success,
        "a clean corpus exits successfully"
    );
    assert_ne!(
        format!("{:?}", cmd_check_indirect_buffers(leaky.path(), false)),
        success,
        "a corpus with violations must NOT exit successfully"
    );
}

/// `ReachCounts::is_vacuous` is the per-arm half of the T5a guard: candidates
/// without tracked buffers is the only shape that means "proved nothing".
#[test]
fn arm_counts_vacuity_needs_candidates_without_tracked() {
    let vacuous = ReachCounts {
        candidates: 3,
        tracked: 0,
    };
    assert!(vacuous.is_vacuous(), "candidates but nothing tracked");

    for healthy in [
        ReachCounts {
            candidates: 0,
            tracked: 0,
        },
        ReachCounts {
            candidates: 3,
            tracked: 1,
        },
    ] {
        assert!(!healthy.is_vacuous(), "{healthy:?}");
    }

    // A vacuous arm poisons the whole summary, whichever arm it is.
    let callee_only = AuditSummary {
        callee: vacuous,
        ..AuditSummary::default()
    };
    let shim_only = AuditSummary {
        shim: vacuous,
        ..AuditSummary::default()
    };
    assert!(callee_only.is_vacuous());
    assert!(shim_only.is_vacuous());
    assert!(!AuditSummary::default().is_vacuous(), "an empty corpus");
}
