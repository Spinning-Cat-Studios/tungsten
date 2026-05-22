//! Tests for `natind` and `natrec` surface syntax (ADR 22.5.26a).

use crate::elaborate::tests::{elab_err, elab_ok};

// ============================================================================
// natrec — positive tests
// ============================================================================

#[test]
fn natrec_basic() {
    elab_ok("fn f(n: Nat) -> Nat { natrec(Nat, 0, fn(k: Nat, acc: Nat) => acc, n) }");
}

// ============================================================================
// natrec — negative tests
// ============================================================================

#[test]
fn natrec_target_not_nat() {
    let errors =
        elab_err("fn f(b: Bool) -> Nat { natrec(Nat, 0, fn(k: Nat, acc: Nat) => acc, b) }");
    assert!(!errors.is_empty());
}

#[test]
fn natrec_base_type_mismatch() {
    let errors =
        elab_err("fn f(n: Nat) -> Nat { natrec(Nat, true, fn(k: Nat, acc: Nat) => acc, n) }");
    assert!(!errors.is_empty());
}

// ============================================================================
// natind — positive tests
// ============================================================================

#[test]
fn natind_basic() {
    elab_ok("fn f(n: Nat) -> Nat { natind(|k: Nat| Nat, 0, fn(k: Nat, ih: Nat) => ih, n) }");
}

// ============================================================================
// natind — negative tests
// ============================================================================

#[test]
fn natind_motive_not_lambda() {
    // Use a numeric literal as motive — it should parse as MotiveExpr then fail in elaboration
    let errors = elab_err("fn f(n: Nat) -> Nat { natind(0, 0, fn(k: Nat, ih: Nat) => ih, n) }");
    assert!(!errors.is_empty());
}

#[test]
fn natind_motive_domain_not_nat() {
    let errors =
        elab_err("fn f(n: Nat) -> Nat { natind(|k: Bool| Nat, 0, fn(k: Nat, ih: Nat) => ih, n) }");
    assert!(!errors.is_empty());
}

#[test]
fn natind_target_not_nat() {
    let errors =
        elab_err("fn f(b: Bool) -> Nat { natind(|k: Nat| Nat, 0, fn(k: Nat, ih: Nat) => ih, b) }");
    assert!(!errors.is_empty());
}

#[test]
fn natind_base_type_mismatch() {
    let errors = elab_err(
        "fn f(n: Nat) -> Nat { natind(|k: Nat| Nat, true, fn(k: Nat, ih: Nat) => ih, n) }",
    );
    assert!(!errors.is_empty());
}
