//! Tests for `null-calls`: the callee-position predicate (ADR 28.7.26e D2),
//! the parser-reach counters, and the directory verdict.

use super::super::corpus::fixtures::corpus_of as corpus_dir;
use super::*;

/// **Provenance: copied from the emitted corpus** —
/// `target/ll-audit/driver/ffi/io/mkdir_p.ll:50`, one of the 15 false findings
/// the substring heuristic reported on the self-hosted compiler's own IR
/// (ADR 28.7.26e §1.2). The callee's *name* ends in `null`; the call does not
/// target a null pointer.
const CORPUS_CSTRING_IS_NULL: &str =
    "  %call7 = call i1 @cstring_is_null(ptr null, i64 %thunk_call)";

#[test]
fn detects_null_call() {
    let content = r#"
define void @foo() {
  %1 = call i64 null(ptr null)
  ret void
}
"#;
    let summary = scan_ll_content("test.ll", content);
    assert_eq!(summary.findings.len(), 1);
    assert_eq!(summary.findings[0].line_num, 3);
}

#[test]
fn no_false_positive_on_normal_call() {
    let content = r#"
define void @foo() {
  %1 = call i64 @bar(i64 42)
  ret void
}
"#;
    assert!(scan_ll_content("test.ll", content).findings.is_empty());
}

#[test]
fn no_false_positive_on_null_store() {
    let content = r#"
define void @foo() {
  store ptr null, ptr %1
  ret void
}
"#;
    let summary = scan_ll_content("test.ll", content);
    assert!(summary.findings.is_empty());
    assert_eq!(
        summary.calls.candidates, 0,
        "a store is not a call instruction"
    );
}

#[test]
fn detects_multiple_null_calls() {
    let content = r#"
  %1 = call i64 null(ptr null)
  %2 = call { ptr, ptr } null(ptr null, i64 1)
"#;
    assert_eq!(scan_ll_content("multi.ll", content).findings.len(), 2);
}

/// The defect, both directions (ADR 28.7.26e AC1): the real corpus line does
/// not flag, a genuine null callee still does.
#[test]
fn a_callee_named_like_null_is_not_a_null_callee() {
    let content = format!(
        "define void @mkdir_p() {{\n{CORPUS_CSTRING_IS_NULL}\n  \
           %1 = call i64 null(ptr null)\n  ret void\n}}\n"
    );
    let summary = scan_ll_content("mkdir_p.ll", &content);
    assert_eq!(
        summary.findings.len(),
        1,
        "only the genuine null callee flags: {:?}",
        summary.findings
    );
    assert_eq!(summary.findings[0].line_num, 3);
    assert_eq!(
        summary.calls,
        ReachCounts {
            candidates: 2,
            tracked: 2
        },
        "both calls were seen and both callees resolved"
    );
}

#[test]
fn comment_with_null_is_false_positive() {
    // Known limitation: a full-line comment is not distinguished from an
    // instruction, so a comment that happens to spell a null call matches.
    // Documented rather than asserted absent — emitted IR does not contain it.
    let content = "; call something null(not real)\n";
    assert_eq!(
        scan_ll_content("comment.ll", content).findings.len(),
        1,
        "heuristic matches comment lines — known limitation"
    );
}

/// **Provenance: copied from the emitted corpus** —
/// `target/ll-audit/codegen/ir_closures/application/emit_app.ll:13`. The
/// self-hosted compiler emits LLVM IR as text, so its own IR carries globals
/// whose contents read like a call; a literal spelling a *null* callee would
/// otherwise be a finding (ADR 28.7.26e).
#[test]
fn ir_text_in_a_module_scope_global_is_not_a_call() {
    let content = concat!(
        "@str_lit.7 = private unnamed_addr constant [9 x i8] c\" = call \\00\", align 1\n",
        "@tmpl = private unnamed_addr constant [24 x i8] c\"  %r = call i64 null(x)\\00\", align 1\n",
    );
    let summary = scan_ll_content("emit_app.ll", content);
    assert!(
        summary.findings.is_empty(),
        "a global's contents are data: {:?}",
        summary.findings
    );
    assert_eq!(
        summary.calls,
        ReachCounts::default(),
        "and they are not even candidates"
    );
}

/// A call the parser cannot resolve a callee for counts as a candidate and not
/// as tracked — the drift signal, rather than a silent skip.
#[test]
fn an_unresolvable_call_line_is_a_candidate_but_not_tracked() {
    let summary = scan_ll_content("drift.ll", "  %r = call i64 <<unparseable>>\n");
    assert_eq!(
        summary.calls,
        ReachCounts {
            candidates: 1,
            tracked: 0
        }
    );
    assert!(summary.calls.is_vacuous(), "strict must fail this corpus");
    assert!(summary.findings.is_empty(), "vacuity is not a violation");
}

// ── The directory verdict ───────────────────────────────────────────────────

/// A directory with no IR in it is bad input, not a pass.
#[test]
fn empty_directory_is_bad_input() {
    let dir = tempfile::TempDir::new().unwrap();
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::EmptyCorpus,
            "strict={strict}: a gate must not go green over nothing"
        );
    }
}

/// An IR file with no call instructions at all: candidates are zero, so there
/// is nothing to prove and `--strict` passes.
#[test]
fn a_corpus_with_no_calls_is_clean_under_both_strictness_settings() {
    let dir = corpus_dir(&["define void @f() {\nentry:\n  ret void\n}\n"]);
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Clean,
            "strict={strict}: no candidates means nothing to prove"
        );
    }
}

/// The corpus shape: real calls, none of them null → clean even under strict.
#[test]
fn the_real_corpus_line_leaves_a_clean_non_vacuous_verdict() {
    let dir = corpus_dir(&[&format!(
        "define void @mkdir_p() {{\n{CORPUS_CSTRING_IS_NULL}\n  ret void\n}}\n"
    )]);
    for strict in [false, true] {
        assert_eq!(audit_directory(dir.path(), strict), AuditVerdict::Clean);
    }
}

#[test]
fn scan_directory_recurses_and_reports_violations() {
    let dir = tempfile::TempDir::new().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("nested.ll"), "  %1 = call i64 null(ptr null)\n").unwrap();
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Violations(1),
            "strict={strict}: the nested subdir is scanned too"
        );
    }
}

/// Drift fails only under `--strict`, and the command carries the verdict
/// through to a process status — an entry point that always succeeded would be
/// a gate that proves nothing.
#[test]
fn vacuous_corpus_fails_only_under_strict() {
    let dir = corpus_dir(&["  %r = call i64 <<unparseable>>\n"]);
    assert_eq!(audit_directory(dir.path(), false), AuditVerdict::Clean);
    assert_eq!(
        audit_directory(dir.path(), true),
        AuditVerdict::VacuousUnderStrict
    );

    let clean = format!("{:?}", ExitCode::SUCCESS);
    assert_eq!(
        format!("{:?}", cmd_check_null_calls(dir.path(), false)),
        clean
    );
    assert_ne!(
        format!("{:?}", cmd_check_null_calls(dir.path(), true)),
        clean,
        "drift under --strict must not exit successfully"
    );
    assert_ne!(
        format!(
            "{:?}",
            cmd_check_null_calls(&dir.path().join("missing"), false)
        ),
        clean,
        "a non-directory must not exit successfully"
    );
}
