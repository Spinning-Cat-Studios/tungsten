//! Tests for hint selection and HintTracker deduplication/suppression.

use super::*;
use crate::ElabErrorKind;
use tungsten_core::Type;

// ─────────────────────────────────────────────────────────────────────────────
// Hint selection
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_select_hints_type_mismatch_returns_at_most_2() {
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    let hints = select_hints(&kind, None);
    assert!(hints.len() <= 2);
    assert!(!hints.is_empty());
}

#[test]
fn test_select_hints_type_mismatch_includes_suggest_tools() {
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    let hints = select_hints(&kind, None);
    assert!(
        hints.iter().any(|h| h.command.contains("suggest-tools")),
        "Expected suggest-tools hint, got: {:?}",
        hints.iter().map(|h| &h.command).collect::<Vec<_>>()
    );
}

#[test]
fn test_select_hints_type_mismatch_with_adt_extracts_name() {
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::Adt("List".to_string(), vec![Type::Nat], vec![]),
        found: Type::Bool,
    };
    let hints = select_hints(&kind, Some(std::path::Path::new("examples/list.tg")));
    let first = &hints[0];
    assert!(
        first.command.contains("List"),
        "Expected type name 'List' in hint, got: {}",
        first.command
    );
    assert!(
        first.command.contains("examples/list.tg"),
        "Expected file path in hint, got: {}",
        first.command
    );
}

#[test]
fn test_select_hints_name_resolution_has_explain() {
    let kind = ElabErrorKind::UndefinedVariable("x".to_string());
    let hints = select_hints(&kind, None);
    assert!(hints.iter().any(|h| h.command.contains("explain error")));
}

#[test]
fn test_select_hints_entry_point_has_suggest_tools() {
    let kind = ElabErrorKind::NoMainFunction;
    let hints = select_hints(&kind, None);
    assert!(hints.iter().any(|h| h.command.contains("suggest-tools")));
}

#[test]
fn test_select_hints_pattern_matching_has_explain() {
    let kind = ElabErrorKind::NonExhaustiveMatch;
    let hints = select_hints(&kind, None);
    assert!(
        hints.iter().any(|h| h.command.contains("explain error")),
        "PatternMatching category should suggest `explain error`, got: {:?}",
        hints.iter().map(|h| &h.command).collect::<Vec<_>>()
    );
}

#[test]
fn test_select_hints_elaboration_has_explain() {
    let kind = ElabErrorKind::UnsupportedFeature("traits".to_string());
    let hints = select_hints(&kind, None);
    assert!(
        hints.iter().any(|h| h.command.contains("explain error")),
        "Elaboration category should suggest `explain error`, got: {:?}",
        hints.iter().map(|h| &h.command).collect::<Vec<_>>()
    );
}

#[test]
fn test_select_hints_general_only_has_suggest_tools() {
    let kind = ElabErrorKind::Other("something weird".to_string());
    let hints = select_hints(&kind, None);
    assert_eq!(
        hints.len(),
        1,
        "General category should have exactly 1 hint (suggest-tools)"
    );
    assert!(hints[0].command.contains("suggest-tools"));
}

#[test]
fn test_select_hints_type_mismatch_with_app_extracts_name() {
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::App("Option".to_string(), vec![Type::Nat]),
        found: Type::Bool,
    };
    let hints = select_hints(&kind, Some(std::path::Path::new("test.tg")));
    assert!(
        hints[0].command.contains("Option"),
        "Expected type name 'Option' in hint, got: {}",
        hints[0].command
    );
}

#[test]
fn test_select_hints_type_mismatch_primitive_uses_placeholder() {
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    let hints = select_hints(&kind, None);
    assert!(
        hints[0].command.contains("<Type>"),
        "Primitive type should use <Type> placeholder, got: {}",
        hints[0].command
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// HintTracker (dedup + category suppression)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_tracker_first_error_passes_through() {
    let mut tracker = HintTracker::new();
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    let hints = select_hints(&kind, None);
    let filtered = tracker.filter_hints(&kind, hints.clone());
    assert_eq!(filtered.len(), hints.len());
}

#[test]
fn test_tracker_second_same_category_suppressed() {
    let mut tracker = HintTracker::new();
    let kind1 = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    let kind2 = ElabErrorKind::TypeMismatch {
        expected: Type::Bool,
        found: Type::Nat,
    };
    let hints1 = select_hints(&kind1, None);
    let hints2 = select_hints(&kind2, None);
    let _ = tracker.filter_hints(&kind1, hints1);
    let filtered = tracker.filter_hints(&kind2, hints2);
    assert!(filtered.is_empty());
}

#[test]
fn test_tracker_different_category_passes() {
    let mut tracker = HintTracker::new();
    let kind1 = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    let kind2 = ElabErrorKind::UndefinedVariable("x".to_string());
    let hints1 = select_hints(&kind1, None);
    let hints2 = select_hints(&kind2, None);
    let _ = tracker.filter_hints(&kind1, hints1);
    let filtered = tracker.filter_hints(&kind2, hints2);
    assert!(!filtered.is_empty());
}

#[test]
fn test_tracker_suppressed_count() {
    let mut tracker = HintTracker::new();
    let kind = ElabErrorKind::TypeMismatch {
        expected: Type::Nat,
        found: Type::Bool,
    };
    let hints1 = select_hints(&kind, None);
    let hints2 = select_hints(&kind, None);
    let count2 = hints2.len();
    let _ = tracker.filter_hints(&kind, hints1);
    let _ = tracker.filter_hints(&kind, hints2);
    assert_eq!(tracker.suppressed_count(), count2);
}

#[test]
fn test_tracker_dedup_identical_commands_across_categories() {
    let mut tracker = HintTracker::new();

    // First: General category
    let kind1 = ElabErrorKind::Other("problem".to_string());
    let hints1 = vec![DiagnosticHint {
        command: "tungsten doctor suggest-tools \"error\"".to_string(),
        reason: "reason".to_string(),
    }];
    let filtered1 = tracker.filter_hints(&kind1, hints1);
    assert_eq!(filtered1.len(), 1);

    // Second: different category but same command string — should be deduped
    let kind2 = ElabErrorKind::NoMainFunction;
    let hints2 = vec![DiagnosticHint {
        command: "tungsten doctor suggest-tools \"error\"".to_string(),
        reason: "reason".to_string(),
    }];
    let filtered2 = tracker.filter_hints(&kind2, hints2);
    assert!(
        filtered2.is_empty(),
        "Identical command across categories should be deduped"
    );
}
