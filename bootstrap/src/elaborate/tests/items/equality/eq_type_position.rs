//! Tests for `Eq<T, a, b>` and `==` sugar type-position syntax (ADR 21.5.26f).

use crate::elaborate::tests::elab_ok;

// ── Eq<T, a, b> explicit form ────────────────────────────────────────────

#[test]
fn eq_explicit_refl_nat() {
    elab_ok("fn f() -> Eq<Nat, 0, 0> { refl }");
}

#[test]
fn eq_explicit_refl_bool() {
    elab_ok("fn f() -> Eq<Bool, true, true> { refl }");
}

#[test]
fn eq_explicit_as_param_type() {
    elab_ok("fn f(p: Eq<Nat, 0, 0>) -> Eq<Nat, 0, 0> { p }");
}

#[test]
fn eq_explicit_let_binding() {
    elab_ok("fn f() -> Eq<Nat, 0, 0> { let x: Eq<Nat, 0, 0> = refl; x }");
}

// ── == sugar ─────────────────────────────────────────────────────────────

#[test]
fn eq_sugar_int() {
    elab_ok("fn f() -> 0 == 0 { refl }");
}

#[test]
fn eq_sugar_bool() {
    elab_ok("fn f() -> true == true { refl }");
}

#[test]
fn eq_sugar_in_let() {
    elab_ok("fn f() -> 0 == 0 { let x: 0 == 0 = refl; x }");
}
