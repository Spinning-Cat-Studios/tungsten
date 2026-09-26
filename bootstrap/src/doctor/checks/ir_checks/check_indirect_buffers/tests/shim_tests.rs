//! Tests for the ADR 17.7.26e audit arms: the `$direct` shim's buffer
//! discipline (I4, `ShimBufferEscape`) and the self-`musttail` edge's
//! buffer-slot distinctness (I5, `TailEdgeAliasedForward`) — the two witnesses
//! that were missing when `noalias` was withheld from the indirect-param slots.

use super::*;

/// The canonical shim `shim.rs::compile_decompose_shim` emits: sret + one
/// indirect buffer, lifetime-marked, filled by a store, handed to `$direct_mt`,
/// then the sret read-back.
const CANONICAL_SHIM: &str = r#"
define { ptr, ptr } @"scan_loop$direct"(ptr %0, { { ptr, i64 }, ptr } %1, ptr %2, i64 %3) {
entry:
  %sret_buf = alloca { ptr, ptr }, align 8
  call void @llvm.lifetime.start.p0(i64 16, ptr %sret_buf)
  %indirect_buf.0 = alloca { { ptr, i64 }, ptr }, align 8
  call void @llvm.lifetime.start.p0(i64 24, ptr %indirect_buf.0)
  store { { ptr, i64 }, ptr } %1, ptr %indirect_buf.0, align 8
  call void @"scan_loop$direct_mt"(ptr noalias nonnull sret({ ptr, ptr }) align 8 dereferenceable(16) %sret_buf, ptr noalias nonnull align 8 dereferenceable(24) %indirect_buf.0, ptr %0, ptr %2, i64 %3)
  call void @llvm.lifetime.end.p0(i64 24, ptr %indirect_buf.0)
  %sret_load = load { ptr, ptr }, ptr %sret_buf, align 8
  call void @llvm.lifetime.end.p0(i64 16, ptr %sret_buf)
  ret { ptr, ptr } %sret_load
}
"#;

/// Rewrite the canonical shim's read-back line into some other use of the
/// buffer, so each negative case differs from the positive one by exactly the
/// instruction under test.
fn shim_with_extra_use(instr: &str) -> String {
    CANONICAL_SHIM.replace(
        "  %sret_load = load { ptr, ptr }, ptr %sret_buf, align 8\n",
        &format!("  %sret_load = load {{ ptr, ptr }}, ptr %sret_buf, align 8\n  {instr}\n"),
    )
}

// ── I4: the shim arm ────────────────────────────────────────────────────────

#[test]
fn canonical_shim_shape_passes() {
    let s = audit_ir_summary(CANONICAL_SHIM);
    assert!(s.findings.is_empty(), "canonical shim must pass: {s:?}");
    assert_eq!(
        s.shim.candidates, 1,
        "attributed $direct_mt call → candidate"
    );
    assert_eq!(s.shim.tracked, 2, "sret_buf + indirect_buf.0 tracked");
    assert!(!s.is_vacuous());
}

#[test]
fn shim_storing_buffer_address_as_a_value_is_flagged() {
    let f = audit_ir(&shim_with_extra_use(
        "store ptr %indirect_buf.0, ptr @global, align 8",
    ));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, FindingKind::ShimBufferEscape);
}

#[test]
fn shim_passing_buffer_to_a_second_call_is_flagged() {
    let f = audit_ir(&shim_with_extra_use(
        "%x = call i64 @some_helper(ptr %indirect_buf.0)",
    ));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, FindingKind::ShimBufferEscape);
}

#[test]
fn shim_returning_the_buffer_pointer_is_flagged() {
    let f = audit_ir(&shim_with_extra_use("ret ptr %sret_buf"));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, FindingKind::ShimBufferEscape);
}

#[test]
fn shim_storing_a_derived_buffer_address_as_a_value_is_flagged() {
    // The escape survives one level of address derivation.
    let bad = shim_with_extra_use(
        "%field = getelementptr inbounds { { ptr, i64 }, ptr }, ptr %indirect_buf.0, i32 0, i32 1",
    );
    let bad = bad.replace(
        "  ret { ptr, ptr } %sret_load\n",
        "  store ptr %field, ptr @global, align 8\n  ret { ptr, ptr } %sret_load\n",
    );
    let f = audit_ir(&bad);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, FindingKind::ShimBufferEscape);
}

#[test]
fn shim_reading_back_an_indirect_buffer_is_flagged() {
    // Loading THROUGH `sret_buf` after the call is the read-back (permitted);
    // the same load through an `indirect_buf.*` is off the shim shape.
    let f = audit_ir(&shim_with_extra_use(
        "%peek = load { { ptr, i64 }, ptr }, ptr %indirect_buf.0, align 8",
    ));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, FindingKind::ShimBufferEscape);
}

#[test]
fn shim_alloca_merely_mentioning_a_buffer_is_not_permitted() {
    // The allowlist's alloca/derivation pass is anchored to the line that
    // *defines* a tracked address. A line that only mentions one — synthetic
    // here, since well-formed IR cannot put a pointer in an alloca's count
    // operand — must not inherit the pass from its opcode alone.
    let f = audit_ir(&shim_with_extra_use("%scratch = alloca i8, ptr %sret_buf"));
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, FindingKind::ShimBufferEscape);
}

#[test]
fn no_sret_shim_shape_is_a_candidate() {
    // The R4 no-sret Class-P shape (scalar return, one indirect buffer): the
    // call carries `dereferenceable(…)` but no `sret(…)`, and is a shim all
    // the same. This is the emitted shape of `spin$direct` in
    // tests/classp_musttail_run.tg.
    let no_sret = r#"
define i64 @"spin$direct"(ptr %0, { { i64, i64 }, i64 } %1, i64 %2) {
entry:
  %indirect_buf.0 = alloca { { i64, i64 }, i64 }, align 8
  call void @llvm.lifetime.start.p0(i64 24, ptr %indirect_buf.0)
  store { { i64, i64 }, i64 } %1, ptr %indirect_buf.0, align 8
  %shim_call = call i64 @"spin$direct_mt"(ptr noalias nonnull align 8 dereferenceable(24) %indirect_buf.0, ptr %0, i64 %2)
  call void @llvm.lifetime.end.p0(i64 24, ptr %indirect_buf.0)
  ret i64 %shim_call
}
"#;
    let s = audit_ir_summary(no_sret);
    assert_eq!(s.shim.candidates, 1, "dereferenceable alone marks a shim");
    assert_eq!(s.shim.tracked, 1);
    assert!(s.findings.is_empty(), "{s:?}");
}

#[test]
fn shim_memcpy_and_debug_intrinsics_over_a_buffer_are_permitted() {
    let good = shim_with_extra_use(
        "call void @llvm.memcpy.p0.p0.i64(ptr %scratch, ptr %sret_buf, i64 16, i1 false)",
    );
    assert!(audit_ir(&good).is_empty(), "{:?}", audit_ir(&good));
}

#[test]
fn non_shim_functions_are_not_shim_candidates() {
    // The closure-returning wrapper calls `$direct`, not `$direct_mt`: not a
    // candidate, so its own allocas are none of the shim arm's business.
    let wrapper = r#"
define ptr @scan_loop(ptr %0) {
entry:
  %sret_buf = alloca { ptr, ptr }, align 8
  store ptr %sret_buf, ptr @env_slot, align 8
  ret ptr %sret_buf
}
"#;
    let s = audit_ir_summary(wrapper);
    assert_eq!(s.shim.candidates, 0, "no $direct_mt call → not a shim");
    assert!(s.findings.is_empty());
}

#[test]
fn decompose_only_shim_without_buffers_is_not_a_candidate() {
    // Shape C (ADR 1.7.26e R8): flattenable struct → decomposed scalars, no
    // buffer slots at the call. Zero buffers is correct here, not vacuous.
    let shape_c = r#"
define i64 @"flat$direct"(ptr %0, { i64, i64 } %1) {
entry:
  %unpack.0.0 = extractvalue { i64, i64 } %1, 0
  %unpack.0.1 = extractvalue { i64, i64 } %1, 1
  %shim_call = call i64 @"flat$direct_mt"(ptr %0, i64 %unpack.0.0, i64 %unpack.0.1)
  ret i64 %shim_call
}
"#;
    let s = audit_ir_summary(shape_c);
    assert_eq!(
        s.shim.candidates, 0,
        "no attributed buffer slot → not a shim"
    );
    assert!(!s.is_vacuous());
}

#[test]
fn renamed_shim_buffers_are_a_vacuous_pass() {
    // ADR 17.7.26e §2.2: candidate detection is name-independent, so a rename
    // in `shim.rs` surfaces as candidates > 0 with nothing tracked — the
    // strict-mode failure that keeps a silent green from replacing the proof.
    let renamed = CANONICAL_SHIM
        .replace("%sret_buf", "%out_slot")
        .replace("%indirect_buf.0", "%arg_slot");
    let s = audit_ir_summary(&renamed);
    assert_eq!(s.shim.candidates, 1, "still recognized as a shim");
    assert_eq!(s.shim.tracked, 0, "renamed buffers are unrecognized");
    assert!(s.findings.is_empty(), "vacuity is not a violation");
    assert!(s.is_vacuous(), "strict mode must fail this corpus");
}

// ── I5: tail-edge buffer-slot distinctness ──────────────────────────────────

/// The canonical self-`musttail` edge: sret + two indirect buffers forwarded
/// positionally, each pointer appearing exactly once.
const DISTINCT_TAIL_EDGE: &str = r#"
define void @"swap2$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr noalias nonnull align 8 dereferenceable(24) %1, ptr noalias nonnull align 8 dereferenceable(24) %2, ptr %3, i64 %4) {
entry:
  store { i64, i64 } zeroinitializer, ptr %0, align 8
  musttail call void @"swap2$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr noalias nonnull align 8 dereferenceable(24) %2, ptr noalias nonnull align 8 dereferenceable(24) %1, ptr null, i64 %4)
  ret void
}
"#;

#[test]
fn distinct_tail_edge_slots_pass() {
    // Swapping the two buffers between slots is fine — they stay distinct.
    let s = audit_ir_summary(DISTINCT_TAIL_EDGE);
    assert!(s.findings.is_empty(), "{s:?}");
    assert_eq!(s.callee.tracked, 3, "sret + both buffers forwarded");
}

#[test]
fn tail_edge_forwarding_one_pointer_into_two_slots_is_flagged() {
    let bad = DISTINCT_TAIL_EDGE.replace(
        "dereferenceable(24) %2, ptr noalias nonnull align 8 dereferenceable(24) %1, ptr null",
        "dereferenceable(24) %1, ptr noalias nonnull align 8 dereferenceable(24) %1, ptr null",
    );
    let f = audit_ir(&bad);
    assert_eq!(
        f.iter()
            .filter(|x| x.kind == FindingKind::TailEdgeAliasedForward)
            .count(),
        1,
        "one aliased-forward finding: {f:?}"
    );
    assert!(f[0].line.contains("%1 forwarded into two buffer slots"));
}

#[test]
fn tail_edge_repeating_a_flat_pointer_arg_is_not_flagged() {
    // A recursive-ADT (`Mu`) argument is a plain `ptr` with no slot attribute;
    // `f(l, l)` passes the same list twice and aliases nothing the ABI promises.
    let good = r#"
define i64 @"walk$direct_mt"(ptr noalias nonnull align 8 dereferenceable(24) %0, ptr %1, ptr %2, ptr %3) {
entry:
  %v = load { { i64, i64 }, i64 }, ptr %0, align 8
  %r = musttail call i64 @"walk$direct_mt"(ptr noalias nonnull align 8 dereferenceable(24) %0, ptr null, ptr %2, ptr %2)
  ret i64 %r
}
"#;
    assert!(audit_ir(good).is_empty(), "{:?}", audit_ir(good));
}

// ── Corpus provenance (ADR 28.7.26e D5) ─────────────────────────────────────

/// **Provenance: copied verbatim from the emitted corpus** —
/// `target/ll-audit/lexer/token/token_new.ll`, the `$direct` shim and its
/// `$direct_mt` callee, attributes and all.
///
/// Every fixture above is hand-written, and hand-written fixtures are exactly
/// how this audit came to report 32 false escapes on its first contact with
/// real IR: they froze at the audit's birth while emitted IR kept evolving
/// (ADR 17.7.26e §6.1). One fixture whose text came *out of the compiler*
/// pins the actual shape — two indirect buffers rather than one, interleaved
/// lifetime intrinsics, and the real sret type spelling.
const CORPUS_TOKEN_NEW: &str = r#"
define { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } } @"token_new$direct"(ptr %0, { i32, [16 x i8] } %1, { i64, { i64, { ptr, i64 } } } %2) {
entry:
  %sret_buf = alloca { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } }, align 8
  call void @llvm.lifetime.start.p0(i64 56, ptr %sret_buf)
  %indirect_buf.0 = alloca { i32, [16 x i8] }, align 8
  call void @llvm.lifetime.start.p0(i64 20, ptr %indirect_buf.0)
  store { i32, [16 x i8] } %1, ptr %indirect_buf.0, align 4
  %indirect_buf.1 = alloca { i64, { i64, { ptr, i64 } } }, align 8
  call void @llvm.lifetime.start.p0(i64 32, ptr %indirect_buf.1)
  store { i64, { i64, { ptr, i64 } } } %2, ptr %indirect_buf.1, align 8
  call void @"token_new$direct_mt"(ptr noalias nonnull sret({ { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } }) align 8 dereferenceable(56) %sret_buf, ptr noalias nonnull align 4 dereferenceable(20) %indirect_buf.0, ptr noalias nonnull align 8 dereferenceable(32) %indirect_buf.1, ptr %0)
  call void @llvm.lifetime.end.p0(i64 20, ptr %indirect_buf.0)
  call void @llvm.lifetime.end.p0(i64 32, ptr %indirect_buf.1)
  %sret_load = load { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } }, ptr %sret_buf, align 8
  call void @llvm.lifetime.end.p0(i64 56, ptr %sret_buf)
  ret { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } } %sret_load
}

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
fn the_emitted_shim_shape_passes_and_is_not_vacuous() {
    let summary = audit_ir_summary(CORPUS_TOKEN_NEW);
    assert!(
        summary.findings.is_empty(),
        "real emitted IR must pass: {:?}",
        summary.findings
    );
    assert_eq!(
        summary.shim,
        ReachCounts {
            candidates: 1,
            tracked: 3
        },
        "the shim's sret + two indirect buffers are all tracked"
    );
    assert!(!summary.shim.is_vacuous());
    // `token_new` is not self-recursive, so its `$direct_mt` forwards nothing
    // across a tail edge: the callee arm counts it as a candidate and tracks
    // zero. That is the arm's designed drift signal, and it is only meaningful
    // over a whole corpus — on the self-hosted compiler's 2,049 files the
    // callee arm tracks 266 buffers across 1,010 candidates.
    assert_eq!(
        summary.callee,
        ReachCounts {
            candidates: 1,
            tracked: 0
        }
    );
}

/// The same corpus shim with one buffer pointer stored as a *value* — the
/// escape the audit exists to catch, on real emitted text rather than a
/// hand-written approximation of it.
#[test]
fn an_escape_injected_into_the_emitted_shim_is_caught() {
    let leaky = CORPUS_TOKEN_NEW.replace(
        "  %sret_load = load { { i32, [16 x i8] }, { i64, { i64, { ptr, i64 } } } }, ptr %sret_buf, align 8\n",
        "  store ptr %indirect_buf.0, ptr @global, align 8\n",
    );
    let findings = audit_ir(&leaky);
    assert!(
        findings
            .iter()
            .any(|f| f.kind == FindingKind::ShimBufferEscape),
        "{findings:?}"
    );
}
