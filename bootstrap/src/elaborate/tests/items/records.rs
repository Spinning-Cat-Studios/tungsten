//! Tests for basic record types and field access.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok};
use tungsten_core::{Term, Type};

// ─────────────────────────────────────────────────────────────────────────────
// Record types
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_record_type_basic() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn make_point(x: Nat, y: Nat) -> Point {
            { x: x, y: y }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "make_point");
    // Record types remain nominal during elaboration (encoding happens at codegen)
    // @-prefix distinguishes named types from genuine type variables (ADR 13.4.26c §2)
    assert_eq!(
        defs[0].ty,
        Type::arrow(
            Type::Nat,
            Type::arrow(Type::Nat, Type::TyVar("@Point".to_string()))
        )
    );
}

#[test]
fn test_elaborate_record_field_access() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn point_x(p: Point) -> Nat {
            p.x
        }
        
        fn point_y(p: Point) -> Nat {
            p.y
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
    // p.x should become fst(p)
    assert!(matches!(defs[0].term.term, Term::Lambda(_, _, _)));
    // p.y should become snd(p)
    assert!(matches!(defs[1].term.term, Term::Lambda(_, _, _)));
}

#[test]
fn test_elaborate_record_out_of_order_fields() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn make_point_reversed(x: Nat, y: Nat) -> Point {
            { y: y, x: x }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    // Should still work - elaborator reorders fields to canonical order
}

#[test]
fn test_elaborate_single_field_record() {
    let defs = elab_ok(
        r#"
        type Wrapper = { inner: String }
        
        fn wrap(s: String) -> Wrapper {
            { inner: s }
        }
        
        fn unwrap(w: Wrapper) -> String {
            w.inner
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
    // Record types remain nominal during elaboration
    // @-prefix distinguishes named types from genuine type variables (ADR 13.4.26c §2)
    assert_eq!(
        defs[0].ty,
        Type::arrow(Type::String, Type::TyVar("@Wrapper".to_string()))
    );
    // Return type is String (field access)
    assert_eq!(
        defs[1].ty,
        Type::arrow(Type::TyVar("@Wrapper".to_string()), Type::String)
    );
}

#[test]
fn test_elaborate_three_field_record() {
    let defs = elab_ok(
        r#"
        type Person = { name: String, age: Nat, active: Bool }
        
        fn person_age(p: Person) -> Nat {
            p.age
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    // Three-field: String × (Nat × Bool)
    // age is field 1 (middle), so access is fst(snd(p))
}

#[test]
fn test_elaborate_record_missing_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn bad() -> Point {
            { x: 10 }
        }
    "#,
    );
    assert!(errors
        .iter()
        .any(|e| { matches!(&e.kind, ElabErrorKind::MissingRecordField { .. }) }));
}

#[test]
fn test_elaborate_record_unknown_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn bad() -> Point {
            { x: 10, y: 20, z: 30 }
        }
    "#,
    );
    assert!(errors
        .iter()
        .any(|e| { matches!(&e.kind, ElabErrorKind::ExtraRecordField { .. }) }));
}

#[test]
fn test_elaborate_record_duplicate_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn bad() -> Point {
            { x: 10, x: 20, y: 30 }
        }
    "#,
    );
    assert!(errors
        .iter()
        .any(|e| { matches!(&e.kind, ElabErrorKind::DuplicateRecordField(_)) }));
}

#[test]
fn test_elaborate_field_access_unknown_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn bad(p: Point) -> Nat {
            p.z
        }
    "#,
    );
    assert!(errors
        .iter()
        .any(|e| { matches!(&e.kind, ElabErrorKind::ExtraRecordField { .. }) }));
}

#[test]
fn test_elaborate_field_access_non_record() {
    let errors = elab_err(
        r#"
        fn bad(n: Nat) -> Nat {
            n.x
        }
    "#,
    );
    assert!(errors
        .iter()
        .any(|e| { matches!(&e.kind, ElabErrorKind::NotARecordType(_)) }));
}

#[test]
fn test_elaborate_record_literal_no_annotation() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn bad() -> Point {
            let p = { x: 10, y: 20 };
            p
        }
    "#,
    );
    // Should error: cannot infer record type
    assert!(!errors.is_empty());
}

#[test]
fn test_elaborate_record_literal_with_annotation() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        
        fn good() -> Point {
            let p: Point = { x: 10, y: 20 };
            p
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}
