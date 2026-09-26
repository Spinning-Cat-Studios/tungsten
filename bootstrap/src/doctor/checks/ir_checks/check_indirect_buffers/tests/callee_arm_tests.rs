//! Tests for the Class-P indirect-buffer derived-address audit (ADR 1.7.26e
//! §2.6 R4/R10): permitted uses (load/store-through, GEP/memcpy/lifetime,
//! the forwarding musttail arg) pass; each escape form is rejected.
//!
//! Every `$direct_mt` fixture carries the **canonical slot attributes** the
//! compiler has emitted since 1.7.26e P6 (`nonnull align dereferenceable`, and
//! `sret(%T)` on slot 0), because those attributes are how the audit tells a
//! buffer slot from a flat `ptr` argument (ADR 17.7.26e P2).

use super::*;

/// The buffer-slot attribute text a Class-P indirect param carries.
const BUF: &str = "ptr nonnull align 8 dereferenceable(24)";

#[test]
fn good_forwarding_has_no_findings() {
    let good = format!(
        r#"
define i64 @"spin$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  %ctx.indirect.load = load {{ {{ i64, i64 }}, i64 }}, ptr %0, align 8
  store {{ {{ i64, i64 }}, i64 }} %ctx.indirect.load, ptr %0, align 8
  %musttail_decomposed = musttail call i64 @"spin$direct_mt"({BUF} %0, ptr null, i64 %2)
  ret i64 %musttail_decomposed
}}
"#
    );
    // `store <agg>, ptr %0` writes THROUGH the buffer — allowed.
    assert!(audit_ir(&good).is_empty(), "clean forwarding must pass");
    assert_eq!(audit_ir_summary(&good).callee.tracked, 1);
}

#[test]
fn forwarded_alloca_is_flagged() {
    // A per-iteration alloca forwarded across the musttail edge (R10 bug).
    let bad = format!(
        r#"
define i64 @"bad$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  %fresh = alloca {{ i64, i64 }}, align 8
  %musttail_decomposed = musttail call i64 @"bad$direct_mt"({BUF} %fresh, ptr null, i64 %2)
  ret i64 %musttail_decomposed
}}
"#
    );
    let f = audit_ir(&bad);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].kind, FindingKind::ForwardedAlloca);
}

#[test]
fn returned_buffer_pointer_is_flagged() {
    // The buffer %0 is forwarded across the musttail AND returned — an escape.
    let bad = format!(
        r#"
define ptr @"leak$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  %r = musttail call ptr @"leak$direct_mt"({BUF} %0, ptr null, i64 %2)
  ret ptr %0
}}
"#
    );
    let f = audit_ir(&bad);
    assert!(
        f.iter().any(|x| x.kind == FindingKind::PointerEscape),
        "got: {f:?}"
    );
}

#[test]
fn stored_buffer_pointer_value_is_flagged() {
    // Storing the buffer pointer VALUE escapes; storing THROUGH it does not.
    let bad = format!(
        r#"
define i64 @"leak2$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  store ptr %0, ptr @global, align 8
  %r = musttail call i64 @"leak2$direct_mt"({BUF} %0, ptr null, i64 %2)
  ret i64 %r
}}
"#
    );
    let f = audit_ir(&bad);
    assert!(
        f.iter().any(|x| x.kind == FindingKind::PointerEscape),
        "got: {f:?}"
    );
}

#[test]
fn non_direct_mt_functions_are_ignored() {
    // `main` storing its argv param is NOT a buffer escape (not a $direct_mt).
    let main = r#"
define i64 @main(ptr %0, ptr %1) {
entry:
  store ptr %1, ptr @__tungsten_argv, align 8
  ret i64 0
}
"#;
    assert!(audit_ir(main).is_empty());
}

// ── R4 derived-address audit (ADR 1.7.26e §2.6/§5) ──────────────────────────

#[test]
fn gep_and_memcpy_over_buffer_are_permitted() {
    // Positive case: field addressing (GEP), whole-aggregate memcpy, and
    // lifetime markers over the buffer are all PERMITTED address uses.
    let good = format!(
        r#"
define i64 @"cp$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  %field = getelementptr inbounds {{ {{ i64, i64 }}, i64 }}, ptr %0, i32 0, i32 1
  %v = load i64, ptr %field, align 8
  call void @llvm.memcpy.p0.p0.i64(ptr %scratch, ptr %0, i64 24, i1 false)
  call void @llvm.lifetime.start.p0(i64 24, ptr %scratch)
  store {{ {{ i64, i64 }}, i64 }} zeroinitializer, ptr %0, align 8
  %r = musttail call i64 @"cp$direct_mt"({BUF} %0, ptr null, i64 %v)
  ret i64 %r
}}
"#
    );
    assert!(
        audit_ir(&good).is_empty(),
        "GEP + memcpy + lifetime over the buffer must pass: {:?}",
        audit_ir(&good)
    );
}

#[test]
fn derived_gep_address_returned_is_flagged() {
    // A GEP-derived interior address of the buffer reaching `ret` is an escape.
    let bad = format!(
        r#"
define ptr @"leak3$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  %field = getelementptr inbounds {{ {{ i64, i64 }}, i64 }}, ptr %0, i32 0, i32 1
  %r = musttail call ptr @"leak3$direct_mt"({BUF} %0, ptr null, i64 %2)
  ret ptr %field
}}
"#
    );
    let f = audit_ir(&bad);
    assert!(
        f.iter().any(|x| x.kind == FindingKind::PointerEscape),
        "got: {f:?}"
    );
}

#[test]
fn buffer_passed_to_non_forwarding_call_is_flagged() {
    // The buffer pointer as an argument to a NON-forwarding call (a callee
    // could retain it) is an escape — unlike the forwarding musttail arg.
    let bad = format!(
        r#"
define i64 @"leak4$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  %x = call i64 @some_helper(ptr %0)
  %r = musttail call i64 @"leak4$direct_mt"({BUF} %0, ptr null, i64 %x)
  ret i64 %r
}}
"#
    );
    let f = audit_ir(&bad);
    assert!(
        f.iter().any(|x| x.kind == FindingKind::PointerEscape),
        "got: {f:?}"
    );
}

#[test]
fn derived_address_stored_as_value_is_flagged() {
    // Storing a DERIVED address as a value escapes the buffer address too.
    let bad = format!(
        r#"
define i64 @"leak5$direct_mt"({BUF} %0, ptr %1, i64 %2) {{
entry:
  %field = getelementptr inbounds {{ {{ i64, i64 }}, i64 }}, ptr %0, i32 0, i32 1
  store ptr %field, ptr @global, align 8
  %r = musttail call i64 @"leak5$direct_mt"({BUF} %0, ptr null, i64 %2)
  ret i64 %r
}}
"#
    );
    let f = audit_ir(&bad);
    assert!(
        f.iter().any(|x| x.kind == FindingKind::PointerEscape),
        "got: {f:?}"
    );
}

/// Regression for the 32 phantom escapes the audit reported the first time it
/// was pointed at a full self-compile corpus (ADR 17.7.26e P2): a **flat**
/// recursive-ADT (`Mu`) argument is a bare `ptr` param with no slot attribute.
/// Forwarding it across the tail edge *and* handing it to a helper is ordinary
/// by-value argument passing, not a buffer escape.
#[test]
fn flat_ptr_argument_handed_to_a_helper_is_not_an_escape() {
    let good = r#"
define void @"module_map_lookup$direct_mt"(ptr noalias nonnull sret({ i32, [48 x i8] }) align 4 dereferenceable(52) %0, ptr %1, ptr %2, ptr %3) {
entry:
  %direct_call = call i1 @"elab_module_path_eq$direct"(ptr null, ptr %fst5, ptr %3)
  musttail call void @"module_map_lookup$direct_mt"(ptr noalias nonnull sret({ i32, [48 x i8] }) align 4 dereferenceable(52) %0, ptr null, ptr %snd12, ptr %3)
  ret void
}
"#;
    let s = audit_ir_summary(good);
    assert!(s.findings.is_empty(), "flat ptr arg is not a buffer: {s:?}");
    assert_eq!(s.callee.tracked, 1, "only the sret buffer is tracked");
}

/// A `musttail` to a **different** function is not the self-edge: it neither
/// satisfies R10's forwarding contract (so a fresh alloca handed to it is not
/// this function's per-iteration-buffer bug) nor excuses handing it a buffer
/// (the callee is not re-entering *this* activation's slot discipline, so the
/// buffer escapes).
#[test]
fn delegating_musttail_is_not_the_self_edge() {
    let delegating = r#"
define void @"leak7$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr nonnull align 8 dereferenceable(24) %1, ptr %2, i1 %3) {
entry:
  br i1 %3, label %recurse, label %delegate

recurse:
  musttail call void @"leak7$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %0, ptr nonnull align 8 dereferenceable(24) %1, ptr null, i1 %3)
  ret void

delegate:
  %fresh = alloca { i64, i64 }, align 8
  musttail call void @"other$direct_mt"(ptr noalias nonnull sret({ i64, i64 }) align 8 dereferenceable(16) %fresh, ptr nonnull align 8 dereferenceable(24) %1, ptr null, i1 %3)
  ret void
}
"#;
    let f = audit_ir(delegating);
    assert_eq!(f.len(), 1, "exactly the escape of %1: {f:?}");
    assert_eq!(
        f[0].kind,
        FindingKind::PointerEscape,
        "the delegated buffer escapes"
    );
    assert!(
        !f.iter().any(|x| x.kind == FindingKind::ForwardedAlloca),
        "%fresh goes to ANOTHER function's edge — not this function's R10 bug: {f:?}"
    );
}
