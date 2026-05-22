//! Tests for named record constructors (ADR 13.5.26h).

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok};
use tungsten_core::Type;

// ── AC2: Named record infers correct type ────────────────────────────

#[test]
fn named_record_basic_infer() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }

        fn make() -> Point {
            Point { x: 7, y: 3 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::TyVar("@Point".to_string()));
}

// ── AC7: Field order is independent ──────────────────────────────────

#[test]
fn named_record_reversed_fields() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }

        fn make() -> Point {
            Point { y: 3, x: 7 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::TyVar("@Point".to_string()));
}

// ── AC8: Named record as function argument (synth mode) ──────────────

#[test]
fn named_record_as_argument() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }

        fn get_x(p: Point) -> Nat { p.x }

        fn main() -> Nat {
            get_x(Point { x: 7, y: 3 })
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
}

// ── AC3: Field type mismatch produces E0010 ──────────────────────────

#[test]
fn named_record_field_type_mismatch() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }

        fn bad() -> Point {
            Point { x: "hello", y: 3 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ElabErrorKind::TypeMismatch { .. })),
        "Expected TypeMismatch error, got: {errors:?}"
    );
}

// ── AC4: Non-record type name produces clear error ───────────────────

#[test]
fn named_record_on_adt_type() {
    let errors = elab_err(
        r#"
        type Color = Red | Green | Blue

        fn bad() -> Color {
            Color { r: 255 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| { matches!(&e.kind, ElabErrorKind::NotARecordType(name) if name == "Color") }),
        "Expected 'not a record type' error, got: {errors:?}"
    );
}

// ── AC5: Missing field produces field-aware error ────────────────────

#[test]
fn named_record_missing_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }

        fn bad() -> Point {
            Point { x: 7 }
        }
    "#,
    );
    assert!(
        errors.iter().any(|e| {
            matches!(&e.kind, ElabErrorKind::MissingRecordField { field, type_name } if field == "y" && type_name == "Point")
        }),
        "Expected MissingRecordField error, got: {errors:?}"
    );
}

// ── AC5: Extra field produces field-aware error ──────────────────────

#[test]
fn named_record_extra_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }

        fn bad() -> Point {
            Point { x: 7, y: 3, z: 1 }
        }
    "#,
    );
    assert!(
        errors.iter().any(|e| {
            matches!(&e.kind, ElabErrorKind::ExtraRecordField { field, type_name } if field == "z" && type_name == "Point")
        }),
        "Expected ExtraRecordField error, got: {errors:?}"
    );
}

// ── AC6: Duplicate field produces clear error ────────────────────────

#[test]
fn named_record_duplicate_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }

        fn bad() -> Point {
            Point { x: 7, x: 3, y: 1 }
        }
    "#,
    );
    assert!(
        errors.iter().any(|e| {
            matches!(&e.kind, ElabErrorKind::DuplicateRecordField(name) if name == "x")
        }),
        "Expected DuplicateRecordField error, got: {errors:?}"
    );
}

// ── Generic record rejection ─────────────────────────────────────────

#[test]
fn named_record_generic_rejected() {
    let errors = elab_err(
        r#"
        type Container<T> = { value: T }

        fn bad() -> Container<Nat> {
            Container { value: 42 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ElabErrorKind::UnsupportedFeature(_))),
        "Expected UnsupportedFeature error for generic record, got: {errors:?}"
    );
}

// ── Undefined type name ──────────────────────────────────────────────

#[test]
fn named_record_undefined_type() {
    let errors = elab_err(
        r#"
        fn bad() -> Nat {
            Missing { x: 1 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ElabErrorKind::UndefinedType(_))),
        "Expected UndefinedType error, got: {errors:?}"
    );
}

// ── Check-mode fallthrough ──────────────────────────────────────────

#[test]
fn named_record_check_mode_fallthrough() {
    // Named record in check mode: synth infers Point, check verifies it matches.
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        fn make() -> Point {
            let p: Point = Point { x: 1, y: 2 };
            p
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::TyVar("@Point".to_string()));
}

// ── Empty record rejected at elaboration ────────────────────────────

#[test]
fn named_record_empty_fields() {
    // Point {} parses but should fail: missing fields x and y.
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        fn bad() -> Point {
            Point {}
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ElabErrorKind::MissingRecordField { .. })),
        "Expected missing field error, got: {errors:?}"
    );
}

// ══════════════════════════════════════════════════════════════════════
// Spread tests (ADR 13.5.26i)
// ══════════════════════════════════════════════════════════════════════

// ── AC7: Named record spread produces correct type ───────────────────

#[test]
fn named_record_spread_basic() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        fn update(p: Point) -> Point {
            Point { ...p, x: 42 }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ── AC6b: Pure spread with no overrides ──────────────────────────────

#[test]
fn named_record_spread_no_overrides() {
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        fn copy(p: Point) -> Point {
            Point { ...p }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

// ── AC4: Spread with non-record type produces error ──────────────────

#[test]
fn named_record_spread_non_record_type() {
    let errors = elab_err(
        r#"
        type Color = Red | Green | Blue
        fn bad(c: Color) -> Color {
            Color { ...c }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ElabErrorKind::NotARecordType(_))),
        "Expected NotARecordType error, got: {errors:?}"
    );
}

// ── AC5c: Duplicate explicit fields with spread ──────────────────────

#[test]
fn named_record_spread_duplicate_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        fn bad(p: Point) -> Point {
            Point { ...p, x: 1, x: 2 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ElabErrorKind::DuplicateRecordField(_))),
        "Expected DuplicateRecordField error, got: {errors:?}"
    );
}

// ── Spread with extra field ──────────────────────────────────────────

#[test]
fn named_record_spread_extra_field() {
    let errors = elab_err(
        r#"
        type Point = { x: Nat, y: Nat }
        fn bad(p: Point) -> Point {
            Point { ...p, z: 1 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ElabErrorKind::ExtraRecordField { .. })),
        "Expected ExtraRecordField error, got: {errors:?}"
    );
}
