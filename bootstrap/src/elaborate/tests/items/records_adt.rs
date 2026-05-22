//! Tests for ADT types with record field types (Gap #3 regression tests).

use crate::elaborate::tests::{elab_err, elab_ok};
use tungsten_core::Type;

// ─────────────────────────────────────────────────────────────────────────────
// ADT with Record Field Types (Gap #3 regression test)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_adt_with_record_field() {
    // Test that ADT variants can contain record types as fields
    // Record types are now preserved as nominal TyVar inside ADT encodings
    let defs = elab_ok(
        r#"
        type Cursor = { pos: Nat, line: Nat }
        type MaybeCursor = NoCursor | SomeCursor(Cursor)
        
        fn make_default_cursor() -> Cursor {
            { pos: 0, line: 1 }
        }
        
        fn get_cursor_or_default(mc: MaybeCursor) -> Cursor {
            match mc {
                NoCursor() => make_default_cursor(),
                SomeCursor(c) => c,
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[1].name, "get_cursor_or_default");
    // MaybeCursor encodes to Unit + TyVar("@Cursor") - records are preserved as nominal
    // @-prefix distinguishes named types from genuine type variables (ADR 13.4.26c §2)
    assert_eq!(
        defs[1].ty,
        Type::arrow(
            Type::sum(Type::Unit, Type::TyVar("@Cursor".to_string())),
            Type::TyVar("@Cursor".to_string())
        )
    );
}

#[test]
fn test_elaborate_adt_with_nested_record() {
    // Test record inside record inside ADT
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        type Line = { start: Point, end: Point }
        type MaybeLine = NoLine | SomeLine(Line)
        
        fn has_line(ml: MaybeLine) -> Bool {
            match ml {
                NoLine() => false,
                SomeLine(_) => true,
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}

#[test]
fn test_elaborate_adt_with_multiple_record_fields() {
    // Test ADT with multiple record-typed fields in same variant
    let defs = elab_ok(
        r#"
        type Point = { x: Nat, y: Nat }
        type Segment = Start(Point) | Middle(Point, Point) | End(Point)
        
        fn segment_start(s: Segment) -> Point {
            match s {
                Start(p) => p,
                Middle(p, _) => p,
                End(p) => p,
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
}
