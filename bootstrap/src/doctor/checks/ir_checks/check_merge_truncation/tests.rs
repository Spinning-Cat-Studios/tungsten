//! Tests for the §6.6 truncating-merge lint (ADR 2.7.26b T6): the REAL
//! captured pre-fix IR sample must be flagged, the post-fix IR must pass, and
//! out-of-supported-shape memcpys must be documented-unmatched.

use super::super::corpus::fixtures::corpus_of as corpus_dir;
use super::*;

/// Real pre-fix IR captured from commit d74f259e~1 compiling
/// `tests/classp_sret_match_order_run.tg` — the actual §6.6 miscompile
/// (`scan_loop`'s ADT merge typed from the musttail arm's `i1` dummy).
///
/// **The `.ll.txt` extension is load-bearing; do not "tidy" it back to `.ll`**
/// (ADR 13.8.26b D1). `.gitignore`'s blanket `*.ll` swallowed these two
/// fixtures, so a fresh clone received `tests.rs` and no `testdata/` — and
/// `include_str!` fails at macro expansion, which makes `cargo test -p
/// tungsten_bootstrap` compile nothing and run *zero* tests rather than fail
/// one. The same pattern is in publish's `EXCLUDE_PATTERNS`, so `.ll` would
/// also strip them from the published snapshot. Nothing reads these files but
/// `include_str!`; the extension only ever bought syntax highlighting. The
/// `embedded-inputs` code-health check is what catches a rename back.
const PREFIX_66: &str = include_str!("testdata/prefix_66_scan_loop.ll.txt");

/// The same fixture compiled by the FIXED compiler. `.ll.txt` for the reason
/// above.
const POSTFIX: &str = include_str!("testdata/postfix_scan_loop.ll.txt");

#[test]
fn captured_prefix_66_sample_is_flagged() {
    let summary = scan_ir(PREFIX_66);
    assert!(
        !summary.matches.is_empty(),
        "the real §6.6 pre-fix IR must be flagged"
    );
    // Both truncation sites: {ptr,ptr}→i1 in the value arm AND i1→{ptr,ptr}
    // at the merge (each a 1-byte memcpy over a 16-byte aggregate).
    assert_eq!(summary.matches.len(), 2, "matches: {:?}", summary.matches);
    for m in &summary.matches {
        assert_eq!(m.copy_size, 1);
        assert_eq!(m.aggregate_type, "{ ptr, ptr }");
        assert_eq!(m.aggregate_size, 16);
        assert!(m.function.contains("scan_loop"), "function: {}", m.function);
    }
}

#[test]
fn postfix_ir_passes() {
    let summary = scan_ir(POSTFIX);
    assert!(
        summary.matches.is_empty(),
        "post-fix IR must be clean: {:?}",
        summary.matches
    );
    assert!(summary.functions() >= 1, "the sample defines a function");
}

#[test]
fn inline_positive_minimal_signature() {
    // Distilled §6.6 shape: 1-byte memcpy reconstructing a {ptr, ptr} whose
    // loaded result feeds the merge phi.
    let ir = r#"
define void @f(ptr %0) {
entry:
  %cast_temp = alloca { ptr, ptr }, align 16
  %src_temp = alloca i1, align 16
  store i1 false, ptr %src_temp, align 16
  %memcpy_cast = call ptr @memcpy(ptr %cast_temp, ptr %src_temp, i64 1)
  %casted = load { ptr, ptr }, ptr %cast_temp, align 16
  br label %merge

merge:
  %result = phi { ptr, ptr } [ %casted, %entry ]
  ret void
}
"#;
    let summary = scan_ir(ir);
    assert_eq!(summary.matches.len(), 1, "{:?}", summary.matches);
    assert_eq!(summary.matches[0].aggregate_size, 16);
}

#[test]
fn full_size_copy_is_not_flagged() {
    // A memcpy that copies the WHOLE aggregate is a legitimate cast.
    let ir = r#"
define void @f(ptr %0) {
entry:
  %cast_temp = alloca { ptr, ptr }, align 16
  %src_temp = alloca { ptr, ptr }, align 16
  %memcpy_cast = call ptr @memcpy(ptr %cast_temp, ptr %src_temp, i64 16)
  %casted = load { ptr, ptr }, ptr %cast_temp, align 16
  br label %merge

merge:
  %result = phi { ptr, ptr } [ %casted, %entry ]
  ret void
}
"#;
    assert!(scan_ir(ir).matches.is_empty());
}

#[test]
fn non_phi_memcpy_is_ignored() {
    // Truncating copy whose result does NOT feed a phi: not the merge shape.
    let ir = r#"
define i64 @f(ptr %0) {
entry:
  %big = alloca { i64, i64, i64 }, align 8
  %small = alloca i64, align 8
  %memcpy_cast = call ptr @memcpy(ptr %small, ptr %big, i64 8)
  %v = load i64, ptr %small, align 8
  ret i64 %v
}
"#;
    assert!(scan_ir(ir).matches.is_empty());
}

#[test]
fn out_of_supported_shape_is_unmatched() {
    // GEP-derived (non-direct-alloca) memcpy operands are OUTSIDE the
    // supported shape and must be unmatched — documented reach (§2.6), the
    // guarantee is T1's cast_to_type hard error, not this lint.
    let ir = r#"
define void @f(ptr %0) {
entry:
  %big = alloca { { ptr, ptr }, i64 }, align 16
  %field = getelementptr inbounds { { ptr, ptr }, i64 }, ptr %big, i32 0, i32 0
  %small = alloca i1, align 16
  %memcpy_cast = call ptr @memcpy(ptr %field, ptr %small, i64 1)
  %casted = load { ptr, ptr }, ptr %field, align 16
  br label %merge

merge:
  %result = phi { ptr, ptr } [ %casted, %entry ]
  ret void
}
"#;
    assert!(
        scan_ir(ir).matches.is_empty(),
        "GEP-derived operands are out of the supported shape"
    );
}

#[test]
fn llvm_memcpy_intrinsic_form_is_matched() {
    let ir = r#"
define void @f(ptr %0) {
entry:
  %cast_temp = alloca { ptr, ptr }, align 16
  %src_temp = alloca i1, align 16
  call void @llvm.memcpy.p0.p0.i64(ptr %cast_temp, ptr %src_temp, i64 1, i1 false)
  %casted = load { ptr, ptr }, ptr %cast_temp, align 16
  br label %merge

merge:
  %result = phi { ptr, ptr } [ %casted, %entry ]
  ret void
}
"#;
    assert_eq!(scan_ir(ir).matches.len(), 1);
}

#[test]
fn function_count_is_reported() {
    let ir = "define void @a() {\n  ret void\n}\ndefine void @b() {\n  ret void\n}\n";
    let summary = scan_ir(ir);
    assert_eq!(summary.functions(), 2);
    assert!(summary.matches.is_empty());
}

// ── approx_store_size ────────────────────────────────────────────────────────

#[test]
fn store_sizes_of_common_shapes() {
    assert_eq!(approx_store_size("{ ptr, ptr }"), Some(16));
    assert_eq!(approx_store_size("i1"), Some(1));
    assert_eq!(approx_store_size("i64"), Some(8));
    assert_eq!(approx_store_size("[4 x i64]"), Some(32));
    assert_eq!(approx_store_size("{ i32, [48 x i8] }"), Some(52));
    // Nested struct with alignment padding: i32 field padded to i64 align.
    assert_eq!(approx_store_size("{ i32, i64 }"), Some(16));
    // Unknown/named types are unmodeled → None (unmatched by design).
    assert_eq!(approx_store_size("%struct.Foo"), None);
}

// ── Corpus provenance (ADR 28.7.26e D5) ─────────────────────────────────────

/// **Provenance: copied verbatim from the emitted corpus** —
/// `target/ll-audit/parser/lib/cursor_advance_safe.ll`.
///
/// This is the shape `collect_facts` has to keep recognizing: an `alloca` /
/// `store` / `load` round-trip feeding a `phi` at a merge — the §6.6 signature
/// minus the truncating memcpy. It is also the reason this lint's reach measure
/// is *facts bound* rather than memcpy→phi pairs: the whole corpus emits five
/// memcpy call sites and none has a literal size operand, so a matched-memcpy
/// counter would read as vacuous on healthy IR (ADR 28.7.26e D4).
const CORPUS_CURSOR_ADVANCE_SAFE: &str = r#"
define { ptr, { i64, { ptr, i64 } } } @cursor_advance_safe(ptr %0, { ptr, { i64, { ptr, i64 } } } %1) {
entry:
  %call = call i1 @cursor_is_eof(ptr null, { ptr, { i64, { ptr, i64 } } } %1)
  br i1 %call, label %then, label %else

then:                                             ; preds = %entry
  br label %merge

else:                                             ; preds = %entry
  %call1 = call { ptr, { i64, { ptr, i64 } } } @cursor_advance(ptr null, { ptr, { i64, { ptr, i64 } } } %1)
  %call_result_alloca = alloca { ptr, { i64, { ptr, i64 } } }, align 16
  store { ptr, { i64, { ptr, i64 } } } %call1, ptr %call_result_alloca, align 16
  %call_result_loaded = load { ptr, { i64, { ptr, i64 } } }, ptr %call_result_alloca, align 16
  br label %merge

merge:                                            ; preds = %else, %then
  %if_result = phi { ptr, { i64, { ptr, i64 } } } [ %1, %then ], [ %call_result_loaded, %else ]
  ret { ptr, { i64, { ptr, i64 } } } %if_result
}
"#;

#[test]
fn the_emitted_alloca_load_phi_shape_binds_facts_and_matches_nothing() {
    let summary = scan_ir(CORPUS_CURSOR_ADVANCE_SAFE);
    assert!(
        summary.matches.is_empty(),
        "no memcpy, so nothing to truncate: {:?}",
        summary.matches
    );
    assert_eq!(summary.functions(), 1);
    // 1 alloca + 1 load + 4 phi tokens. `record_phi_inputs` deliberately
    // over-approximates: it takes every `%` token in the incoming list, so the
    // block labels (`%then`, `%else`) join the values (`%1`,
    // `%call_result_loaded`). It is only ever used for a membership test, so a
    // superset is safe. The exact total matters here because 1+1+4 = 6 while
    // 1×1×4 = 4 — a fold that combined the kinds multiplicatively would differ.
    assert_eq!(
        summary.facts.tracked, 6,
        "one alloca + one load + the phi's four tokens, summed"
    );
    assert!(
        !summary.facts.is_vacuous(),
        "real emitted IR must not read as parser drift"
    );
}

/// A body with **no allocas** still has reach: loads and phi inputs are facts
/// too. The zero component is the point — the reach measure must *sum* the three
/// kinds, since a product would report zero (and so vacuity, and so a red
/// `--strict`) for any body missing one of them.
#[test]
fn reach_sums_the_fact_kinds_rather_than_combining_them_multiplicatively() {
    let no_allocas = r#"
define i64 @merge_only(ptr %0, i64 %1) {
entry:
  %v = load i64, ptr %0, align 8
  br label %merge

merge:                                            ; preds = %entry
  %r = phi i64 [ %v, %entry ]
  ret i64 %r
}
"#;
    let summary = scan_ir(no_allocas);
    assert_eq!(
        (summary.facts.candidates, summary.facts.tracked),
        (1, 3),
        "zero allocas + one load + the phi's two tokens (%v, %entry) — summed \
         that is 3; multiplied it would be 0, i.e. vacuous"
    );
    assert!(
        !summary.facts.is_vacuous(),
        "a body the parser reads perfectly well must not read as drift"
    );
}

/// Inject the truncating memcpy into the corpus shape: the alloca'd 32-byte
/// aggregate copied 1 byte, with the loaded result reaching the merge phi.
#[test]
fn a_truncating_memcpy_injected_into_the_emitted_shape_is_matched() {
    let poisoned = CORPUS_CURSOR_ADVANCE_SAFE.replace(
        "  %call_result_loaded = load { ptr, { i64, { ptr, i64 } } }, ptr %call_result_alloca, align 16\n",
        "  %src = alloca { ptr, { i64, { ptr, i64 } } }, align 16\n  \
           call void @llvm.memcpy.p0.p0.i64(ptr %call_result_alloca, ptr %src, i64 1, i1 false)\n  \
           %call_result_loaded = load { ptr, { i64, { ptr, i64 } } }, ptr %call_result_alloca, align 16\n",
    );
    let matches = scan_ir(&poisoned).matches;
    assert_eq!(matches.len(), 1, "{matches:?}");
    assert_eq!(matches[0].copy_size, 1);
    assert_eq!(matches[0].function, "cursor_advance_safe");
}

// ── The directory verdict (ADR 28.7.26e) ────────────────────────────────────

#[test]
fn the_emitted_corpus_shape_is_clean_under_both_strictness_settings() {
    let dir = corpus_dir(&[CORPUS_CURSOR_ADVANCE_SAFE]);
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Clean,
            "strict={strict}"
        );
    }
}

#[test]
fn a_truncating_memcpy_in_a_nested_dir_is_a_violation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("nested");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("poison.ll"), PREFIX_66).unwrap();
    // The captured §6.6 sample truncates twice in `scan_loop$direct_mt`.
    for strict in [false, true] {
        assert_eq!(
            audit_directory(dir.path(), strict),
            AuditVerdict::Violations(2),
            "strict={strict}: the nested subdir is scanned too"
        );
    }
}

/// A function the fact-collector binds nothing in: candidates > 0, tracked == 0.
/// If `collect_facts` stopped recognizing `alloca`/`load`/`phi` the whole corpus
/// would look like this, and the matcher would silently match nothing forever.
#[test]
fn a_corpus_with_no_bindable_facts_fails_only_under_strict() {
    let dir = corpus_dir(&["define void @bare() {\nentry:\n  ret void\n}\n"]);
    let summary = scan_ir("define void @bare() {\nentry:\n  ret void\n}\n");
    assert_eq!(
        (summary.facts.candidates, summary.facts.tracked),
        (1, 0),
        "one function scanned, no facts bound"
    );
    assert!(summary.facts.is_vacuous());

    assert_eq!(audit_directory(dir.path(), false), AuditVerdict::Clean);
    assert_eq!(
        audit_directory(dir.path(), true),
        AuditVerdict::VacuousUnderStrict
    );

    // …and the command carries the verdict through to a process status.
    let clean = format!("{:?}", std::process::ExitCode::SUCCESS);
    assert_eq!(
        format!("{:?}", cmd_check_merge_truncation(dir.path(), false)),
        clean
    );
    assert_ne!(
        format!("{:?}", cmd_check_merge_truncation(dir.path(), true)),
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
