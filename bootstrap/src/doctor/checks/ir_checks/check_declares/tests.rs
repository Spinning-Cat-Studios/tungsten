//! Tests for `tungsten doctor check declares`.

use super::super::corpus::fixtures::corpus_at as corpus_dir;
use super::scanner::{find_missing_declarations, scan_declarations};
use crate::doctor::checks::ir_checks::corpus::{AuditVerdict, ReachCounts};

/// **Provenance: copied from the emitted corpus** —
/// `target/ll-audit/codegen/ir_main/emit_main_wrapper.ll`, trimmed to the
/// module-scope constants and one body line. The self-hosted compiler emits
/// LLVM IR *as text*, so its own IR holds string literals that read like call
/// instructions; scanning them produced all five of this audit's corpus
/// findings, every one false (ADR 28.7.26e).
const CORPUS_IR_TEXT_STRING_LITERALS: &str = "\
@str_lit.3 = private unnamed_addr constant [53 x i8] c\"  %result = call i64 @tungsten_main$direct(ptr null)\\00\", align 1
@str_lit.44 = private unnamed_addr constant [30 x i8] c\" = call i1 @tg_string_eq(ptr \\00\", align 1

declare ptr @malloc(i64)

define i64 @emit_main_wrapper(ptr %0) {
entry:
  %1 = call ptr @malloc(i64 16)
  ret i64 0
}
";

/// A well-formed IR file with all calls declared — should produce no errors.
const VALID_IR: &str = "\
declare ptr @malloc(i64)
declare void @free(ptr)

define i64 @main$direct(ptr %0) {
entry:
  %1 = call ptr @malloc(i64 16)
  call void @free(ptr %1)
  %2 = call i64 @helper$direct(ptr %0)
  ret i64 %2
}

define i64 @helper$direct(ptr %0) {
entry:
  ret i64 42
}
";

/// IR with a missing declaration — should flag the call to @missing_fn.
const BROKEN_IR: &str = "\
declare ptr @malloc(i64)

define i64 @main$direct(ptr %0) {
entry:
  %1 = call ptr @malloc(i64 16)
  %2 = call i64 @missing_fn(ptr %0)
  ret i64 %2
}
";

/// IR containing LLVM intrinsics — should NOT flag them.
const INTRINSIC_IR: &str = "\
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)
declare void @llvm.lifetime.start.p0(i64, ptr)

define void @copy_helper(ptr %dst, ptr %src) {
entry:
  call void @llvm.memcpy.p0.p0.i64(ptr %dst, ptr %src, i64 8, i1 false)
  call void @llvm.lifetime.start.p0(i64 8, ptr %dst)
  ret void
}
";

/// IR with indirect calls through function pointers — should NOT flag them.
const INDIRECT_CALL_IR: &str = "\
define void @call_through_ptr(ptr %fptr) {
entry:
  call void %fptr()
  ret void
}
";

#[test]
fn valid_ir_no_missing() {
    let missing = find_missing_declarations(VALID_IR);
    assert!(
        missing.is_empty(),
        "expected no missing declarations, got: {:?}",
        missing.iter().map(|m| &m.symbol).collect::<Vec<_>>()
    );
}

#[test]
fn broken_ir_reports_missing_symbol() {
    let missing = find_missing_declarations(BROKEN_IR);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].symbol, "missing_fn");
    // Line 6: `%2 = call i64 @missing_fn(ptr %0)`
    assert_eq!(missing[0].line_number, 6);
}

#[test]
fn intrinsics_are_ignored() {
    let missing = find_missing_declarations(INTRINSIC_IR);
    assert!(
        missing.is_empty(),
        "LLVM intrinsics should not be flagged, got: {:?}",
        missing.iter().map(|m| &m.symbol).collect::<Vec<_>>()
    );
}

#[test]
fn indirect_calls_are_ignored() {
    let missing = find_missing_declarations(INDIRECT_CALL_IR);
    assert!(
        missing.is_empty(),
        "indirect calls should not be flagged, got: {:?}",
        missing.iter().map(|m| &m.symbol).collect::<Vec<_>>()
    );
}

/// IR text living in module-scope string constants is not an instruction
/// (ADR 28.7.26e): the corpus line that produced a false finding must not.
#[test]
fn ir_text_in_string_literals_is_not_a_call() {
    let missing = find_missing_declarations(CORPUS_IR_TEXT_STRING_LITERALS);
    assert!(
        missing.is_empty(),
        "string literals are data, not calls, got: {:?}",
        missing.iter().map(|m| &m.symbol).collect::<Vec<_>>()
    );
}

/// The reach pair: targets seen vs targets resolved.
#[test]
fn scan_reports_targets_seen_and_resolved() {
    assert_eq!(
        scan_declarations(VALID_IR).targets,
        ReachCounts {
            candidates: 3,
            tracked: 3
        },
        "three body calls, all resolved"
    );
    assert_eq!(
        scan_declarations(BROKEN_IR).targets,
        ReachCounts {
            candidates: 2,
            tracked: 1
        },
        "the unresolved target is a candidate that did not resolve"
    );
    assert_eq!(
        scan_declarations(CORPUS_IR_TEXT_STRING_LITERALS).targets,
        ReachCounts {
            candidates: 1,
            tracked: 1
        },
        "only the body's @malloc call counts"
    );
}

// ── The directory verdict ───────────────────────────────────────────────────

#[test]
fn exit_code_failure_on_missing() {
    let dir = corpus_dir(&[("broken.ll", BROKEN_IR)]);
    assert_eq!(
        super::audit_directory(dir.path(), false),
        AuditVerdict::Violations(1)
    );
    assert_eq!(
        super::cmd_check_declares(dir.path(), false),
        std::process::ExitCode::FAILURE
    );
}

#[test]
fn exit_code_success_on_valid() {
    let dir = corpus_dir(&[("valid.ll", VALID_IR)]);
    for strict in [false, true] {
        assert_eq!(
            super::audit_directory(dir.path(), strict),
            AuditVerdict::Clean,
            "strict={strict}"
        );
    }
    assert_eq!(
        super::cmd_check_declares(dir.path(), true),
        std::process::ExitCode::SUCCESS
    );
}

#[test]
fn multiple_files_scanned() {
    let dir = corpus_dir(&[("a.ll", VALID_IR), ("b.ll", BROKEN_IR)]);
    assert_eq!(
        super::audit_directory(dir.path(), false),
        AuditVerdict::Violations(1)
    );
}

/// An IR file with no calls at all has no candidates, so there is nothing to
/// prove and `--strict` passes.
#[test]
fn a_corpus_with_no_call_targets_is_clean_under_both_strictness_settings() {
    let dir = corpus_dir(&[("empty.ll", "define void @f() {\nentry:\n  ret void\n}\n")]);
    for strict in [false, true] {
        assert_eq!(
            super::audit_directory(dir.path(), strict),
            AuditVerdict::Clean,
            "strict={strict}: no candidates means nothing to prove"
        );
    }
}

/// This audit's reach pair is **complementary to its findings**: every target
/// seen either resolves (`tracked`) or is reported missing, so
/// `tracked + missing == candidates` always holds.
///
/// The consequence is worth stating rather than papering over: `declares` can
/// never report vacuity *and* be clean — a corpus where nothing resolved is a
/// corpus full of violations, and violations outrank vacuity. Its `--strict`
/// arm is therefore reachable only through unreadable files (ADR 28.7.26e D4),
/// which is exactly why unreadable files are charged as candidates instead of
/// being skipped. This test pins the invariant so a future change to either
/// counter cannot quietly break the reasoning.
#[test]
fn targets_resolved_plus_missing_always_equals_targets_seen() {
    for ir in [
        VALID_IR,
        BROKEN_IR,
        INTRINSIC_IR,
        INDIRECT_CALL_IR,
        CORPUS_IR_TEXT_STRING_LITERALS,
    ] {
        let scan = scan_declarations(ir);
        assert_eq!(
            scan.targets.tracked + scan.missing.len(),
            scan.targets.candidates,
            "every target seen either resolves or is reported: {ir}"
        );
    }
}

/// A directory with no IR in it is bad input, not a pass — the "green over
/// nothing" failure this ADR exists to prevent.
#[test]
fn empty_directory_is_bad_input() {
    let dir = tempfile::TempDir::new().unwrap();
    for strict in [false, true] {
        assert_eq!(
            super::audit_directory(dir.path(), strict),
            AuditVerdict::EmptyCorpus,
            "strict={strict}"
        );
    }
    assert_ne!(
        super::cmd_check_declares(dir.path(), false),
        std::process::ExitCode::SUCCESS
    );
    assert_eq!(
        super::audit_directory(&dir.path().join("missing"), false),
        AuditVerdict::NotADirectory
    );
}
