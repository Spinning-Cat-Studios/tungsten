//! Unit tests for constructor invariant validator (ADR 7.5.26e).

use super::validator::{
    validate_constructors, validate_constructors_with_expected, ConstructorViolation,
};
use crate::elaborate::Constructor;
use tungsten_core::Type;

fn make_ctor(name: &str, index: usize, fields: Vec<Type>) -> Constructor {
    Constructor {
        name: name.to_string(),
        fields,
        index,
        visibility: None,
        span: Default::default(),
    }
}

// ── Invariant 1: entry count ────────────────────────────────────────

#[test]
fn valid_two_constructors() {
    let ctors = vec![
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
    ];
    let result = validate_constructors("AB", &ctors);
    assert!(result.is_ok());
    assert_eq!(result.expected_count, 2);
    assert_eq!(result.actual_count, 2);
}

#[test]
fn count_mismatch_with_explicit_expected() {
    // Simulate duplicate registration: 4 entries for 2 constructors
    let ctors = vec![
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
    ];
    let result = validate_constructors_with_expected("AB", &ctors, 2);
    assert!(!result.is_ok());
    assert!(result
        .violations
        .contains(&ConstructorViolation::CountMismatch {
            expected: 2,
            actual: 4,
        }));
}

// ── Invariant 2: unique indices ─────────────────────────────────────

#[test]
fn duplicate_indices_detected() {
    // 3 entries for index 0
    let ctors = vec![
        make_ctor("A", 0, vec![]),
        make_ctor("A", 0, vec![]),
        make_ctor("A", 0, vec![]),
    ];
    let result = validate_constructors_with_expected("T", &ctors, 1);
    let has_dup_index = result.violations.iter().any(|v| {
        matches!(
            v,
            ConstructorViolation::DuplicateIndex { index: 0, count: 3 }
        )
    });
    assert!(
        has_dup_index,
        "expected duplicate index violation: {:?}",
        result.violations
    );
}

// ── Invariant 3: contiguous indices ─────────────────────────────────

#[test]
fn non_contiguous_indices_detected() {
    // Index 0 and 2, missing 1
    let ctors = vec![make_ctor("A", 0, vec![]), make_ctor("C", 2, vec![])];
    // Note: with only 2 unique names, expected_count=2, so indices should be 0,1
    // but we have 0,2 → missing index 1
    let result = validate_constructors("T", &ctors);
    let has_gap = result.violations.iter().any(|v| {
        matches!(
            v,
            ConstructorViolation::NonContiguousIndices { missing } if missing.contains(&1)
        )
    });
    assert!(
        has_gap,
        "expected non-contiguous indices violation: {:?}",
        result.violations
    );
}

#[test]
fn contiguous_indices_pass() {
    let ctors = vec![
        make_ctor("None", 0, vec![]),
        make_ctor("Some", 1, vec![Type::Nat]),
    ];
    let result = validate_constructors("Option", &ctors);
    assert!(result.is_ok());
}

// ── Invariant 4: unique names ───────────────────────────────────────

#[test]
fn duplicate_names_detected() {
    let ctors = vec![
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
    ];
    let result = validate_constructors("AB", &ctors);
    let has_dup = result.violations.iter().any(|v| {
        matches!(
            v,
            ConstructorViolation::DuplicateName { name, count: 2 } if name == "A"
        )
    });
    assert!(
        has_dup,
        "expected duplicate name violation: {:?}",
        result.violations
    );
}

// ── Invariant 5: parent type (structural) ───────────────────────────

#[test]
fn parent_type_structurally_guaranteed() {
    // This test documents that parent-type consistency is structural:
    // constructors come from adt_types[name], so all belong to that ADT.
    let ctors = vec![make_ctor("X", 0, vec![])];
    let result = validate_constructors("MyType", &ctors);
    assert!(result.is_ok());
}

// ── Compound: duplicate registration scenario from 7.5.26a ──────────

#[test]
fn duplicate_registration_scenario() {
    // The 7.5.26a bug: 6 entries for 2 constructors (3 registrations × 2)
    let ctors = vec![
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
    ];
    let result = validate_constructors_with_expected("AB", &ctors, 2);
    assert!(!result.is_ok());

    // Should report count mismatch
    assert!(result.violations.iter().any(|v| matches!(
        v,
        ConstructorViolation::CountMismatch {
            expected: 2,
            actual: 6
        }
    )));

    // Should report duplicate names
    assert!(result.violations.iter().any(|v| matches!(
        v,
        ConstructorViolation::DuplicateName { name, count: 3 } if name == "A"
    )));
    assert!(result.violations.iter().any(|v| matches!(
        v,
        ConstructorViolation::DuplicateName { name, count: 3 } if name == "B"
    )));
}

// ── Edge: empty constructor list ────────────────────────────────────

#[test]
fn empty_constructors_valid() {
    let ctors: Vec<Constructor> = vec![];
    let result = validate_constructors("Void", &ctors);
    assert!(result.is_ok());
    assert_eq!(result.expected_count, 0);
    assert_eq!(result.actual_count, 0);
}

// ── Grouping output ─────────────────────────────────────────────────

#[test]
fn grouped_output_correct() {
    let ctors = vec![
        make_ctor("A", 0, vec![Type::Nat]),
        make_ctor("B", 1, vec![Type::Nat]),
        make_ctor("A", 0, vec![Type::Nat]),
    ];
    let result = validate_constructors("AB", &ctors);
    // Should have 2 groups: (A, 0) ×2 and (B, 1) ×1
    assert_eq!(result.grouped.len(), 2);
    assert_eq!(result.grouped[0], ("A".to_string(), 0, 2));
    assert_eq!(result.grouped[1], ("B".to_string(), 1, 1));
}
