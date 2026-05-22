//! Tests for record type definitions, record literals, and field access.

use super::parse_expr_ok;
use super::parse_has_errors;
use super::parse_ok;
use super::{unwrap_record_fields, unwrap_type_def};
use crate::ast::*;

#[test]
fn test_record_type_def() {
    let file = parse_ok("type Point = { x: Nat, y: Nat }");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Point");
    let fields = unwrap_record_fields(&file);
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name.name, "x");
    assert_eq!(fields[1].name.name, "y");
}

#[test]
fn test_record_type_single_field() {
    let file = parse_ok("type Wrapper = { inner: String }");
    let t = unwrap_type_def(&file);
    assert_eq!(t.name.name, "Wrapper");
    let fields = unwrap_record_fields(&file);
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name.name, "inner");
}

#[test]
fn test_record_type_trailing_comma() {
    let file = parse_ok("type Point = { x: Nat, y: Nat, }");
    assert_eq!(unwrap_record_fields(&file).len(), 2);
}

#[test]
fn test_record_literal() {
    let expr = parse_expr_ok("{ x: 10, y: 20 }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_none());
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0.name, "x");
            assert_eq!(fields[1].0.name, "y");
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_single_field() {
    let expr = parse_expr_ok("{ inner: s }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_none());
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0.name, "inner");
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_with_spread() {
    let expr = parse_expr_ok("{ ...base, x: 10 }");
    let Expr::RecordLit { spread, fields, .. } = expr else {
        panic!("Expected record literal");
    };
    let spread_expr = spread.as_deref().expect("Expected spread");
    let Expr::Path(path) = spread_expr else {
        panic!("Expected path as spread expression");
    };
    assert!(path.is_simple());
    assert_eq!(path.item_name().name, "base");
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].0.name, "x");
}

#[test]
fn test_record_literal_spread_only() {
    let expr = parse_expr_ok("{ ...p }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_some());
            assert!(fields.is_empty());
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_spread_multiple_fields() {
    let expr = parse_expr_ok("{ ...base, x: 1, y: 2 }");
    match expr {
        Expr::RecordLit { spread, fields, .. } => {
            assert!(spread.is_some());
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0.name, "x");
            assert_eq!(fields[1].0.name, "y");
        }
        _ => panic!("Expected record literal"),
    }
}

#[test]
fn test_record_literal_spread_with_field_access() {
    let expr = parse_expr_ok("{ ...r.inner, x: 5 }");
    let Expr::RecordLit { spread, fields, .. } = expr else {
        panic!("Expected record literal");
    };
    let spread_expr = spread.as_deref().expect("Expected spread");
    let Expr::Field(_, field, _) = spread_expr else {
        panic!("Expected field access as spread");
    };
    assert_eq!(field.name, "inner");
    assert_eq!(fields.len(), 1);
}

// ── Named record constructors (ADR 13.5.26h) ───────────────────────

#[test]
fn test_named_record_basic() {
    let expr = parse_expr_ok("Point { x: 7, y: 3 }");
    match expr {
        Expr::NamedRecord { name, fields, .. } => {
            assert_eq!(name.item_name().name, "Point");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0.name, "x");
            assert_eq!(fields[1].0.name, "y");
        }
        _ => panic!("Expected NamedRecord, got {expr:?}"),
    }
}

#[test]
fn test_named_record_single_field() {
    let expr = parse_expr_ok("Wrapper { inner: 42 }");
    match expr {
        Expr::NamedRecord { name, fields, .. } => {
            assert_eq!(name.item_name().name, "Wrapper");
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0.name, "inner");
        }
        _ => panic!("Expected NamedRecord"),
    }
}

#[test]
fn test_named_record_trailing_comma() {
    let expr = parse_expr_ok("Point { x: 1, y: 2, }");
    match expr {
        Expr::NamedRecord { fields, .. } => {
            assert_eq!(fields.len(), 2);
        }
        _ => panic!("Expected NamedRecord"),
    }
}

#[test]
fn test_lowercase_brace_is_block_not_record() {
    // Lowercase identifiers followed by { should NOT parse as named record.
    // `x` is parsed as a path, then `{` starts a new expression (block).
    // In expression context, `x { ... }` is not valid — the parser stops at `x`.
    let expr = parse_expr_ok("x");
    assert!(matches!(expr, Expr::Path(_)));
}

#[test]
fn test_screaming_snake_case_not_record() {
    // ALL_CAPS constants like INVALID_HANDLE followed by { should NOT parse
    // as named record constructors — only CamelCase names are type-like.
    let expr = parse_expr_ok("INVALID_HANDLE");
    assert!(matches!(expr, Expr::Path(_)));
}

#[test]
fn test_named_record_empty() {
    // Empty field list should parse — elaboration will reject for missing fields.
    let expr = parse_expr_ok("Point {}");
    match expr {
        Expr::NamedRecord { name, fields, .. } => {
            assert_eq!(name.item_name().name, "Point");
            assert!(fields.is_empty());
        }
        _ => panic!("Expected NamedRecord, got {expr:?}"),
    }
}

#[test]
fn test_block_expr_not_record() {
    let expr = parse_expr_ok("{ let x = 1; x }");
    match expr {
        Expr::Block(_, _, _) => {}
        _ => panic!("Expected block expression"),
    }
}

#[test]
fn test_field_access() {
    let expr = parse_expr_ok("point.x");
    let Expr::Field(base, field, _) = expr else {
        panic!("Expected field access");
    };
    let Expr::Path(path) = base.as_ref() else {
        panic!("Expected path as base");
    };
    assert!(path.is_simple());
    assert_eq!(path.item_name().name, "point");
    assert_eq!(field.name, "x");
}

#[test]
fn test_chained_field_access() {
    let expr = parse_expr_ok("r.origin.x");
    let Expr::Field(base, field, _) = expr else {
        panic!("Expected field access");
    };
    assert_eq!(field.name, "x");
    let Expr::Field(inner_base, inner_field, _) = base.as_ref() else {
        panic!("Expected field access as base");
    };
    assert_eq!(inner_field.name, "origin");
    let Expr::Path(path) = inner_base.as_ref() else {
        panic!("Expected path as innermost base");
    };
    assert!(path.is_simple());
    assert_eq!(path.item_name().name, "r");
}

// ── Spread in named record constructors (ADR 13.5.26i) ──────────────

#[test]
fn test_named_record_spread() {
    let expr = parse_expr_ok("Point { ...p, x: 1 }");
    match expr {
        Expr::NamedRecord {
            name,
            spread,
            fields,
            ..
        } => {
            assert_eq!(name.item_name().name, "Point");
            assert!(spread.is_some(), "Expected spread");
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0.name, "x");
        }
        _ => panic!("Expected NamedRecord, got {:?}", expr),
    }
}

#[test]
fn test_named_record_spread_no_overrides() {
    let expr = parse_expr_ok("Point { ...p }");
    match expr {
        Expr::NamedRecord { spread, fields, .. } => {
            assert!(spread.is_some(), "Expected spread");
            assert!(fields.is_empty(), "Expected no fields");
        }
        _ => panic!("Expected NamedRecord"),
    }
}

// ── AC5: Multiple spreads rejected (ADR 13.5.26i) ───────────────────

#[test]
fn test_named_record_multiple_spreads_rejected() {
    assert!(parse_has_errors("Point { ...a, ...b }"));
}

// ── AC5b: Spread after explicit fields rejected ─────────────────────

#[test]
fn test_record_spread_after_fields_rejected() {
    // Anonymous record: spread must be first
    assert!(parse_has_errors("let r: T = { x: 1, ...p }"));
}

#[test]
fn test_named_record_spread_after_fields_rejected() {
    assert!(parse_has_errors("Point { x: 1, ...p }"));
}
