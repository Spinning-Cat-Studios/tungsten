//! Tests for export validation (public item leak detection).

use super::{elab_err, elab_ok};
use crate::elaborate::error::ElabErrorKind;

// ─────────────────────────────────────────────────────────────────────────────
// Export Validation Tests (Phase 3: Public Item Leak Detection)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_export_validation_private_type_in_public_fn_return() {
    // A public function returning a private type should error
    let result = elab_err(
        r#"
        type PrivateType = Nat
        
        pub fn foo() -> PrivateType {
            42
        }
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_private_type_in_public_fn_param() {
    // A public function with a private type parameter should error
    let result = elab_err(
        r#"
        type PrivateType = Nat
        
        pub fn foo(x: PrivateType) -> Nat {
            x
        }
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_private_type_alias_chain() {
    // A public function returning a private type alias should error
    // Note: We test via function signature because `type X = Y` where Y is an identifier
    // is parsed as an ADT, not a type alias (parser ambiguity).
    let result = elab_err(
        r#"
        type PrivateType = Nat -> Nat
        
        pub fn foo() -> PrivateType {
            |x| x
        }
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_pub_crate_leaking_private() {
    // A pub(crate) function returning a private type should also error
    let result = elab_err(
        r#"
        type PrivateType = Nat
        
        pub(crate) fn foo() -> PrivateType {
            42
        }
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_private_type_in_public_adt() {
    // A public ADT with a private type in a constructor field should error
    let result = elab_err(
        r#"
        type PrivateType = Nat
        
        pub type PublicADT = Wrapper(PrivateType)
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_private_type_in_public_record() {
    // A public record with a private type in a field should error
    let result = elab_err(
        r#"
        type PrivateType = Nat
        
        pub type PublicRecord = { field: PrivateType }
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_private_type_in_public_theorem() {
    // A public theorem with a private type in its signature should error
    let result = elab_err(
        r#"
        type PrivateType = Nat
        
        pub theorem my_thm(x: PrivateType) : Prop {
            sorry
        }
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_nested_private_type() {
    // A public function with a nested private type in its signature should error
    // Note: Using arrow type since tuples have different syntax in Tungsten
    let result = elab_err(
        r#"
        type PrivateType = Nat
        
        pub fn foo() -> PrivateType -> Bool {
            |_x| true
        }
    "#,
    );
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_valid_all_public() {
    // A public function returning a public type should be fine
    let defs = elab_ok(
        r#"
        pub type PublicType = Nat
        
        pub fn foo() -> PublicType {
            42
        }
    "#,
    );
    assert_eq!(defs.len(), 1); // Just the function
}

#[test]
fn test_export_validation_valid_private_fn_private_type() {
    // A private function can use private types freely
    let defs = elab_ok(
        r#"
        type PrivateType = Nat
        
        fn foo() -> PrivateType {
            42
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_export_validation_valid_pub_crate_with_pub_crate() {
    // A pub(crate) function can use pub(crate) types
    let defs = elab_ok(
        r#"
        pub(crate) type CrateType = Nat
        
        pub(crate) fn foo() -> CrateType {
            42
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_export_validation_valid_builtin_types() {
    // Public functions using built-in types should always be valid
    let defs = elab_ok(
        r#"
        pub fn foo() -> Nat { 42 }
        pub fn bar() -> Bool { true }
        pub fn baz() -> Unit { () }
        pub fn qux() -> String { "hello" }
    "#,
    );
    assert_eq!(defs.len(), 4);
}

#[test]
fn test_export_validation_transitive_alias_chain() {
    // Test that public function using a private intermediate alias is caught
    // Note: Direct type alias chains are hard to test due to parser ambiguity,
    // so we test via function return type which clearly uses the type.
    let result = elab_err(
        r#"
        type PrivateBase = Nat -> Bool
        type PrivateAlias = PrivateBase
        
        pub fn foo() -> PrivateAlias {
            |_x| true
        }
    "#,
    );
    // This should error because foo is public but returns PrivateAlias
    assert!(result
        .iter()
        .any(|e| matches!(&e.kind, ElabErrorKind::PublicItemLeak { .. })));
}

#[test]
fn test_export_validation_pub_fn_with_pub_type() {
    // A public function using a public generic type should be fine
    let defs = elab_ok(
        r#"
        pub type Option<T> = None | Some(T)
        
        pub fn maybe_nat() -> Option<Nat> {
            Some(42)
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}
