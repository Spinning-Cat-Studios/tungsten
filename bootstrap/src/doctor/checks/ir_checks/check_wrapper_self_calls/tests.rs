//! Tests for `wrapper-self-calls`: the self-correlation, the closure-pair
//! discriminator (ADR 28.7.26e §2.3), the reach counters, and the verdict.

use super::super::corpus::fixtures::corpus_of as corpus_dir;
use super::*;

const WRAPPER: &str = "_tg_12spin_generic_I_3Nat";

/// **Provenance: copied from the emitted corpus** — `target/ll-audit/__mono.ll`
/// lines 8489/8545, the single finding the audit reported on the self-hosted
/// compiler's own IR, triaged in ADR 28.7.26e §2.3 as a **false positive**.
///
/// `list_last<String>` is an arity-1 instance: the corpus contains no
/// `…$direct` twin for it, the define returns the result aggregate rather than
/// a closure pair, and the body allocates no environment. Its self-call is a
/// plain saturated direct call — correct, and must not flag.
const CORPUS_ARITY1_SATURATED_SELF_CALL: &str = r#"
define { i32, [16 x i8] } @_tg_6driver_4util_5lists_9list_last_I_6String(ptr %0, ptr %1) {
entry:
  %call = call { i32, [16 x i8] } @_tg_6driver_4util_5lists_9list_last_I_6String(ptr null, ptr %snd)
  ret { i32, [16 x i8] } %call
}
"#;

#[test]
fn flags_direct_body_calling_its_wrapper() {
    // Pre-fix shape: the $direct body re-enters the closure wrapper.
    let content = format!(
        "define i64 @\"{WRAPPER}$direct\"(ptr %0, i64 %1) {{\n  \
           %call = call {{ ptr, ptr }} @{WRAPPER}(ptr null, i64 %1)\n  \
           ret i64 0\n}}\n"
    );
    let summary = scan_ll_content("mono.ll", &content);
    assert_eq!(summary.findings.len(), 1);
    assert_eq!(
        summary.findings[0].define_symbol,
        format!("{WRAPPER}$direct")
    );
    assert_eq!(summary.findings[0].wrapper_symbol, WRAPPER);
    // The call is on line 2 (define=1, call=2): pins the 1-indexed `i + 1`.
    assert_eq!(summary.findings[0].line_num, 2);
}

#[test]
fn clean_direct_self_recursion_not_flagged() {
    // Post-fix shape: the $direct body musttail-calls its own $direct.
    let content = format!(
        "define i64 @\"{WRAPPER}$direct\"(ptr %0, i64 %1) {{\n  \
           %r = musttail call i64 @\"{WRAPPER}$direct\"(ptr null, i64 %1)\n  \
           ret i64 %r\n}}\n"
    );
    assert!(scan_ll_content("mono.ll", &content).findings.is_empty());
}

#[test]
fn wrapper_body_self_recursion_flagged() {
    // The wrapper's own body recursing into itself.
    let content = format!(
        "define {{ ptr, ptr }} @{WRAPPER}(ptr %0, i64 %1) {{\n  \
           %call = call {{ ptr, ptr }} @{WRAPPER}(ptr null, i64 %1)\n  \
           ret {{ ptr, ptr }} zeroinitializer\n}}\n"
    );
    let summary = scan_ll_content("mono.ll", &content);
    assert_eq!(summary.findings.len(), 1);
    assert_eq!(summary.findings[0].define_symbol, WRAPPER);
}

#[test]
fn higher_order_call_to_other_instances_wrapper_not_flagged() {
    // `main` builds a closure over the instance and calls its wrapper —
    // legitimate higher-order use, a different (non-instance) define block.
    let content = format!(
        "define i64 @tungsten_main(ptr %0) {{\n  \
           %c = call {{ ptr, ptr }} @{WRAPPER}(ptr null, i64 3)\n  \
           ret i64 0\n}}\n"
    );
    assert!(
        scan_ll_content("main.ll", &content).findings.is_empty(),
        "a call from a non-instance body is legitimate higher-order use"
    );
}

/// The ADR 28.7.26e §2.3 triage, as a regression test: an arity-1 instance
/// whose one saturated symbol recurses into itself returns its **result
/// aggregate**, not a closure pair, so it allocates no environment and is not a
/// finding. This is the corpus's only reported finding, and it was false.
#[test]
fn arity1_saturated_self_call_is_not_a_wrapper_reentry() {
    let summary = scan_ll_content("__mono.ll", CORPUS_ARITY1_SATURATED_SELF_CALL);
    assert!(
        summary.findings.is_empty(),
        "the return type is the result aggregate, not {{ ptr, ptr }}: {:?}",
        summary.findings
    );
    assert_eq!(
        summary.bodies,
        ReachCounts {
            candidates: 1,
            tracked: 1
        },
        "the body was still entered and its call still examined"
    );
}

/// `@W` appearing among the *arguments* is not a re-entry — the callee comes
/// from the callee position, not from a substring search.
#[test]
fn passing_the_wrapper_as_an_argument_is_not_a_reentry() {
    let content = format!(
        "define {{ ptr, ptr }} @{WRAPPER}(ptr %0, i64 %1) {{\n  \
           %c = call {{ ptr, ptr }} @apply(ptr @{WRAPPER}, i64 %1)\n  \
           ret {{ ptr, ptr }} %c\n}}\n"
    );
    assert!(scan_ll_content("mono.ll", &content).findings.is_empty());
}

#[test]
fn non_instance_define_ignored() {
    // A plain function whose body calls a plain function.
    let content = "define i64 @foo(ptr %0) {\n  \
           %r = call i64 @bar(ptr null)\n  ret i64 %r\n}\n";
    let summary = scan_ll_content("f.ll", content);
    assert!(summary.findings.is_empty());
    assert_eq!(
        summary.bodies,
        ReachCounts::default(),
        "a non-instance body is not a candidate, so it cannot read as vacuous"
    );
}

#[test]
fn quoted_and_unquoted_define_names_both_parse() {
    assert_eq!(
        defined_symbol("define i64 @\"a$direct\"(ptr %0) {").as_deref(),
        Some("a$direct")
    );
    assert_eq!(
        defined_symbol("define { ptr, ptr } @plain(ptr %0) {").as_deref(),
        Some("plain")
    );
    assert_eq!(defined_symbol("  %1 = call i64 @x()").as_deref(), None);
}

#[test]
fn wrapper_base_strips_suffixes_and_requires_instance_marker() {
    assert_eq!(wrapper_base("w_I_x$direct_mt"), Some("w_I_x"));
    assert_eq!(wrapper_base("w_I_x$direct"), Some("w_I_x"));
    assert_eq!(wrapper_base("w_I_x"), Some("w_I_x"));
    assert_eq!(wrapper_base("plain$direct"), None);
}

/// An instance body containing no call at all is a candidate nothing was
/// examined in — the drift signal `--strict` acts on.
#[test]
fn an_instance_body_with_no_calls_reads_as_vacuous() {
    let content = format!("define i64 @{WRAPPER}(ptr %0) {{\n  ret i64 0\n}}\n");
    let summary = scan_ll_content("mono.ll", &content);
    assert_eq!(
        summary.bodies,
        ReachCounts {
            candidates: 1,
            tracked: 0
        }
    );
    assert!(summary.bodies.is_vacuous());
}

// ── The directory verdict ───────────────────────────────────────────────────

#[test]
fn the_corpus_shape_is_clean_under_both_strictness_settings() {
    let dir = corpus_dir(&[CORPUS_ARITY1_SATURATED_SELF_CALL]);
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Clean,
            "strict={strict}"
        );
    }
}

#[test]
fn a_wrapper_self_call_in_a_nested_dir_is_a_violation() {
    let dir = tempfile::TempDir::new().unwrap();
    let sub = dir.path().join("nested");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(
        sub.join("mono.ll"),
        format!(
            "define i64 @\"{WRAPPER}$direct\"(ptr %0, i64 %1) {{\n  \
               %c = call {{ ptr, ptr }} @{WRAPPER}(ptr null, i64 %1)\n  \
               ret i64 0\n}}\n"
        ),
    )
    .unwrap();
    assert_eq!(
        audit_directory(dir.path(), false),
        AuditVerdict::Violations(1)
    );
}

#[test]
fn vacuity_fails_only_under_strict_and_the_command_carries_the_verdict() {
    let dir = corpus_dir(&[&format!(
        "define i64 @{WRAPPER}(ptr %0) {{\n  ret i64 0\n}}\n"
    )]);
    assert_eq!(audit_directory(dir.path(), false), AuditVerdict::Clean);
    assert_eq!(
        audit_directory(dir.path(), true),
        AuditVerdict::VacuousUnderStrict
    );

    let clean = format!("{:?}", ExitCode::SUCCESS);
    assert_eq!(
        format!("{:?}", cmd_check_wrapper_self_calls(dir.path(), false)),
        clean
    );
    assert_ne!(
        format!("{:?}", cmd_check_wrapper_self_calls(dir.path(), true)),
        clean,
        "drift under --strict must not exit successfully"
    );
    assert_ne!(
        format!(
            "{:?}",
            cmd_check_wrapper_self_calls(std::path::Path::new("/nonexistent/xyz-23726c"), false)
        ),
        clean,
        "a missing directory must not exit successfully"
    );
}
