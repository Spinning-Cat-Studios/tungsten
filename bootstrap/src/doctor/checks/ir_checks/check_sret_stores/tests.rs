//! Unit tests for the sret-return canonical-shape lint (ADR 3.7.26d §5).

use super::super::corpus::fixtures::corpus_of as corpus_dir;
use super::*;

fn kinds(text: &str) -> Vec<SretFindingKind> {
    audit_ir(text).findings.iter().map(|f| f.kind).collect()
}

/// The exact ADR 3.7.26a defect-2 shape: an sret function whose `ret void`
/// follows a plain `call` that discards its result.
#[test]
fn flags_bare_call_ret_void() {
    let ir = r#"
define void @"walk$direct_mt"(ptr noalias nonnull sret({ i32, [8 x i8] }) align 8 %0, ptr %1, i64 %2) {
entry:
  br label %case_left19
case_left19:
  %direct_call = call { i32, [8 x i8] } @"walk$direct"(ptr null, ptr %1, i64 %2)
  ret void
}
"#;
    let summary = audit_ir(ir);
    assert_eq!(summary.sret_functions(), 1);
    assert_eq!(summary.findings.len(), 1, "{:?}", summary.findings);
    assert_eq!(summary.findings[0].kind, SretFindingKind::BareReturn);
    assert_eq!(summary.findings[0].function, "walk$direct_mt");
    assert_eq!(summary.findings[0].line, "ret void");
}

/// The canonical epilogue: store through the sret param, then `ret void`.
#[test]
fn passes_store_form() {
    let ir = r#"
define void @"walk$direct_mt"(ptr sret({ i32, [8 x i8] }) %0, ptr %1) {
entry:
  %v = load { i32, [8 x i8] }, ptr %1
  store { i32, [8 x i8] } %v, ptr %0, align 8
  ret void
}
"#;
    assert!(kinds(ir).is_empty(), "{:?}", audit_ir(ir).findings);
    assert_eq!(audit_ir(ir).sret_functions(), 1);
}

/// The musttail self-tail: the epilogue forwards the out-pointer.
#[test]
fn passes_musttail_forward_form() {
    let ir = r#"
define void @"walk$direct_mt"(ptr noalias nonnull sret({ i32, [8 x i8] }) align 8 %0, ptr %1, i64 %2) {
else10:
  musttail call void @"walk$direct_mt"(ptr noalias nonnull sret({ i32, [8 x i8] }) align 8 %0, ptr %1, i64 %2)
  ret void
}
"#;
    assert!(kinds(ir).is_empty(), "{:?}", audit_ir(ir).findings);
}

/// A musttail that does NOT forward the sret param is still a finding.
#[test]
fn flags_musttail_not_forwarding_sret() {
    let ir = r#"
define void @"walk$direct_mt"(ptr sret({ i64, i64 }) %0, ptr %1) {
entry:
  musttail call void @"other$direct_mt"(ptr %1)
  ret void
}
"#;
    assert_eq!(kinds(ir), vec![SretFindingKind::BareReturn]);
}

/// Non-sret functions are outside the lint entirely — bare returns are fine.
#[test]
fn ignores_non_sret_functions() {
    let ir = r#"
define i64 @main(ptr %0) {
entry:
  %r = call i64 @helper(ptr null)
  ret i64 %r
}
define void @side_effect(ptr %0) {
entry:
  call void @print(ptr %0)
  ret void
}
"#;
    let summary = audit_ir(ir);
    assert_eq!(summary.sret_functions(), 0);
    assert!(summary.findings.is_empty(), "{:?}", summary.findings);
}

/// "sret" in the function NAME only (no sret attribute) is not an sret function.
#[test]
fn sret_in_name_only_is_not_a_candidate() {
    let ir = r#"
define void @make_sret_helper(ptr %0) {
entry:
  call void @print(ptr %0)
  ret void
}
"#;
    let summary = audit_ir(ir);
    assert_eq!(summary.sret_functions(), 0);
    assert!(summary.findings.is_empty(), "{:?}", summary.findings);
}

/// A `ret void` that OPENS its block (label immediately before it) is bare.
#[test]
fn flags_ret_void_at_block_start() {
    let ir = r#"
define void @"f$direct_mt"(ptr sret(%T) %out) {
entry:
  store %T zeroinitializer, ptr %out
  br label %done
done:                       ; preds = %entry
  ret void
}
"#;
    assert_eq!(
        kinds(ir),
        vec![SretFindingKind::BareReturn],
        "store in a predecessor block is flagged by design (canonical-shape lint)"
    );
}

// ── Parsing-contract variants (ADR 3.7.26d §2.1) ─────────────────────────

/// Named sret param + `sret(%T)` named-type spelling, unquoted fn name.
#[test]
fn named_param_and_named_type_spelling() {
    let ir = r#"
define void @emit(ptr sret(%SomeStruct) align 8 %out, i64 %n) {
entry:
  store %SomeStruct zeroinitializer, ptr %out
  ret void
}
"#;
    let summary = audit_ir(ir);
    assert_eq!(summary.sret_functions(), 1);
    assert!(summary.findings.is_empty(), "{:?}", summary.findings);
}

/// The sret attribute in a non-leading parameter position still binds.
#[test]
fn sret_param_in_later_position() {
    let ir = r#"
define void @f(ptr %env, ptr nonnull sret({ i64, i64 }) %ret_slot) {
entry:
  call void @helper(ptr %env)
  ret void
}
"#;
    let summary = audit_ir(ir);
    assert_eq!(summary.sret_functions(), 1);
    assert_eq!(kinds(ir), vec![SretFindingKind::BareReturn]);
}

/// Store with alignment + metadata suffixes still matches the epilogue form.
#[test]
fn store_with_alignment_and_metadata() {
    let ir = r#"
define void @"g$direct_mt"(ptr sret({ i64, i64 }) %0, ptr %1) {
entry:
  store { i64, i64 } zeroinitializer, ptr %0, align 8, !dbg !7
  ret void, !dbg !8
}
"#;
    assert!(kinds(ir).is_empty(), "{:?}", audit_ir(ir).findings);
}

/// A store through a DIFFERENT pointer does not cover the return.
#[test]
fn store_to_other_pointer_is_bare() {
    let ir = r#"
define void @"h$direct_mt"(ptr sret({ i64, i64 }) %0, ptr %1) {
entry:
  store { i64, i64 } zeroinitializer, ptr %1, align 8
  ret void
}
"#;
    assert_eq!(kinds(ir), vec![SretFindingKind::BareReturn]);
}

/// Comments and blank lines between the store and the ret are skipped.
#[test]
fn comments_between_store_and_ret_are_skipped() {
    let ir = r#"
define void @"i$direct_mt"(ptr sret(%T) %0) {
entry:
  store %T zeroinitializer, ptr %0

  ; epilogue
  ret void
}
"#;
    assert!(kinds(ir).is_empty(), "{:?}", audit_ir(ir).findings);
}

/// An sret-mentioning header whose param name cannot be bound fails CLOSED:
/// reported as unparseable, never silently skipped.
#[test]
fn unparseable_sret_header_fails_closed() {
    // No %-named parameter token after the sret attribute (declare-style
    // nameless param snuck into a define — outside the supported grammar).
    let ir = r#"
define void @"weird$direct_mt"(ptr sret({ i64, i64 })) {
entry:
  ret void
}
"#;
    let summary = audit_ir(ir);
    assert_eq!(summary.sret_functions(), 0);
    assert_eq!(kinds(ir), vec![SretFindingKind::UnparseableHeader]);
}

/// Multiple `ret void`s are audited independently — one finding each.
#[test]
fn each_ret_void_audited_independently() {
    let ir = r#"
define void @"j$direct_mt"(ptr sret({ i64, i64 }) %0, i1 %c) {
entry:
  br i1 %c, label %a, label %b
a:
  store { i64, i64 } zeroinitializer, ptr %0
  ret void
b:
  %x = call i64 @helper()
  ret void
}
"#;
    assert_eq!(kinds(ir), vec![SretFindingKind::BareReturn]);
}

// ── Corpus provenance (ADR 28.7.26e D5) ─────────────────────────────────────

/// **Provenance: copied verbatim from the emitted corpus** —
/// `target/ll-audit/lexer/token/token_new.ll`, the `$direct_mt` entry.
///
/// The fixtures above are hand-written, and hand-written fixtures drift: they
/// freeze at the lint's birth while emitted IR keeps changing (ADR 28.7.26e
/// §1.3). This one pins the real header spelling — a nested aggregate sret type
/// carrying interior commas, `noalias nonnull … dereferenceable(56)` attributes
/// ahead of the parameter name, and the canonical `store …; ret void` epilogue.
const CORPUS_TOKEN_NEW_DIRECT_MT: &str = r#"
define void @"token_new$direct_mt"(ptr noalias nonnull sret({ { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } }) align 8 dereferenceable(56) %0, ptr noalias nonnull align 4 dereferenceable(20) %1, ptr noalias nonnull align 8 dereferenceable(32) %2, ptr %3) {
entry:
  %kind.indirect.load = load { i32, [16 x i8] }, ptr %1, align 4
  %span.indirect.load = load { i64, { i64, { ptr, i64 } } }, ptr %2, align 8
  %pair_fst = insertvalue { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } } zeroinitializer, { i32, [16 x i8] } %kind.indirect.load, 0
  %pair_snd = insertvalue { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } } %pair_fst, { i64, { i64, { ptr, i64 } } } %span.indirect.load, 1
  store { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } } %pair_snd, ptr %0, align 8
  ret void
}
"#;

#[test]
fn the_emitted_sret_epilogue_passes_and_is_not_vacuous() {
    let summary = audit_ir(CORPUS_TOKEN_NEW_DIRECT_MT);
    assert!(
        summary.findings.is_empty(),
        "real emitted IR must pass: {:?}",
        summary.findings
    );
    assert_eq!(
        summary.returns,
        crate::doctor::checks::ir_checks::corpus::ReachCounts {
            candidates: 1,
            tracked: 1
        },
        "the header bound its sret param and its one `ret void` was classified"
    );
    assert!(!summary.returns.is_vacuous());
}

/// The same corpus function with its epilogue store removed — the ADR 3.7.26a
/// defect-2 shape, on real emitted text.
#[test]
fn dropping_the_emitted_epilogue_store_is_caught() {
    let bare = CORPUS_TOKEN_NEW_DIRECT_MT.replace(
        "  store { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } } %pair_snd, ptr %0, align 8\n",
        "",
    );
    let findings = audit_ir(&bare).findings;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].kind, SretFindingKind::BareReturn);
}

// ── The directory verdict (ADR 28.7.26e) ────────────────────────────────────

use crate::doctor::checks::ir_checks::corpus::AuditVerdict;

#[test]
fn the_emitted_corpus_shape_is_clean_under_both_strictness_settings() {
    let dir = corpus_dir(&[CORPUS_TOKEN_NEW_DIRECT_MT]);
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Clean,
            "strict={strict}"
        );
    }
}

#[test]
fn a_bare_return_in_a_nested_dir_is_a_violation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("nested");
    std::fs::create_dir(&sub).unwrap();
    let bare = CORPUS_TOKEN_NEW_DIRECT_MT.replace(
        "  store { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } } %pair_snd, ptr %0, align 8\n",
        "",
    );
    std::fs::write(sub.join("bare.ll"), &bare).unwrap();
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Violations(1),
            "strict={strict}: the nested subdir is scanned too"
        );
    }
}

/// An sret function the lint classifies no return in — its body ends in
/// `unreachable`. Candidates > 0, tracked == 0: if the `ret void` spelling ever
/// changed, the whole corpus would look like this and the lint would prove
/// nothing while reporting success.
#[test]
fn a_corpus_with_no_classified_returns_fails_only_under_strict() {
    let no_returns = r#"
define void @"diverge$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr %1) {
entry:
  unreachable
}
"#;
    let summary = audit_ir(no_returns);
    assert!(summary.findings.is_empty(), "{:?}", summary.findings);
    assert_eq!(
        (summary.returns.candidates, summary.returns.tracked),
        (1, 0),
        "an sret function whose returns were never classified"
    );
    assert!(summary.returns.is_vacuous());

    let dir = corpus_dir(&[no_returns]);
    assert_eq!(audit_directory(dir.path(), false), AuditVerdict::Clean);
    assert_eq!(
        audit_directory(dir.path(), true),
        AuditVerdict::VacuousUnderStrict
    );

    // …and the command carries the verdict through to a process status.
    let clean = format!("{:?}", ExitCode::SUCCESS);
    assert_eq!(
        format!("{:?}", cmd_check_sret_stores(dir.path(), false)),
        clean
    );
    assert_ne!(
        format!("{:?}", cmd_check_sret_stores(dir.path(), true)),
        clean,
        "drift under --strict must not exit successfully"
    );
}

#[test]
fn empty_and_missing_directories_are_bad_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        audit_directory(dir.path(), false),
        AuditVerdict::EmptyCorpus,
        "a gate must not go green over nothing"
    );
    assert_eq!(
        audit_directory(&dir.path().join("missing"), false),
        AuditVerdict::NotADirectory
    );
}
