//! Tests for item visibility (basic visibility declarations).
//!
//! Export validation tests are in visibility_export.rs.

use super::{elab_err, elab_ok};
use crate::elaborate::error::ElabErrorKind;

// ─────────────────────────────────────────────────────────────────────────────
// Item Visibility Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_visibility_public_function() {
    // Public function can be called from same module
    let defs = elab_ok(
        r#"
        pub fn public_fn() -> Nat { 42 }
        fn caller() -> Nat { public_fn() }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_visibility_private_function_same_module() {
    // Private function can be called from same module
    let defs = elab_ok(
        r#"
        fn private_fn() -> Nat { 42 }
        fn caller() -> Nat { private_fn() }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_visibility_pub_crate_function() {
    // pub(crate) function can be called from same module
    let defs = elab_ok(
        r#"
        pub(crate) fn internal_fn() -> Nat { 42 }
        fn caller() -> Nat { internal_fn() }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_visibility_public_type() {
    // Public type can be used from same module
    let defs = elab_ok(
        r#"
        pub type PublicPoint = { x: Nat, y: Nat }
        fn make_point() -> PublicPoint { { x: 1, y: 2 } }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_visibility_private_type_same_module() {
    // Private type can be used from same module
    let defs = elab_ok(
        r#"
        type PrivatePoint = { x: Nat, y: Nat }
        fn make_point() -> PrivatePoint { { x: 1, y: 2 } }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_visibility_public_adt() {
    // Public ADT and its constructors can be used from same module
    let defs = elab_ok(
        r#"
        pub type Option<T> = None | Some(T)
        fn make_some(x: Nat) -> Option<Nat> { Some(x) }
        fn make_none() -> Option<Nat> { None }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_visibility_public_theorem() {
    // Public theorem can be referenced from same module
    // Note: 'refl' is a reserved keyword, use 'reflexive' instead
    let defs = elab_ok(
        r#"
        pub theorem reflexive : Nat { 0 }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_visibility_pub_crate_type() {
    // pub(crate) type can be used from same module
    let defs = elab_ok(
        r#"
        pub(crate) type InternalConfig = { debug: Bool }
        fn make_config() -> InternalConfig { { debug: true } }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_visibility_public_extern_fn() {
    // Public extern function can be called from same module
    let defs = elab_ok(
        r#"
        pub extern fn external_fn() -> Nat
        fn caller() -> Nat { external_fn() }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

#[test]
fn test_visibility_mixed_definitions() {
    // Mix of visibility levels in same module
    let defs = elab_ok(
        r#"
        pub type PublicList<T> = Nil | Cons(T, PublicList<T>)
        pub(crate) fn internal_helper() -> Nat { 0 }
        fn private_worker() -> Nat { internal_helper() }
        pub fn public_api() -> PublicList<Nat> { Cons(private_worker(), Nil) }
    "#,
    );
    assert_eq!(defs.len(), 3);
}

#[test]
fn test_visibility_axiom() {
    // Public axiom can be declared
    let defs = elab_ok(
        r#"
        pub axiom my_axiom : Nat
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// Note: Tests for cross-module visibility (private items not accessible from other modules)
// require multi-module elaboration which is not yet fully implemented in the test harness.
// Those tests will be added when multi-module test infrastructure is in place.
