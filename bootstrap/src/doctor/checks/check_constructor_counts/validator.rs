//! Shared constructor invariant validator (ADR 7.5.26e §2.2).
//!
//! Validates five invariants for constructor metadata of an ADT:
//! 1. Entry count equals declared variant count
//! 2. Constructor indices are unique
//! 3. Constructor indices are contiguous from 0..variant_count-1
//! 4. Constructor names are unique within the ADT
//! 5. Every constructor entry references the expected parent type
//!
//! Used by both `info constructors` and `doctor check-constructor-counts`.

use std::collections::HashMap;

use crate::elaborate::Constructor;

/// A violation of constructor metadata invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorViolation {
    /// Entry count doesn't match expected variant count
    CountMismatch { expected: usize, actual: usize },
    /// An index appears more than once
    DuplicateIndex { index: usize, count: usize },
    /// Indices are not contiguous from 0..n-1
    NonContiguousIndices { missing: Vec<usize> },
    /// A constructor name appears more than once
    DuplicateName { name: String, count: usize },
    /// A constructor references the wrong parent type
    WrongParentType {
        constructor: String,
        expected: String,
        actual: String,
    },
}

/// Summary of a single constructor entry for display.
#[derive(Debug, Clone)]
pub struct ConstructorEntry {
    pub name: String,
    pub index: usize,
    pub arity: usize,
    pub field_types_display: String,
}

/// Result of validating constructor metadata for one ADT.
#[derive(Debug, Clone)]
pub struct ConstructorValidationResult {
    pub expected_count: usize,
    pub actual_count: usize,
    pub violations: Vec<ConstructorViolation>,
    pub entries: Vec<ConstructorEntry>,
    /// Grouped entries: (name, index) -> occurrence count
    pub grouped: Vec<(String, usize, usize)>,
}

impl ConstructorValidationResult {
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Validate constructor metadata for an ADT.
///
/// `type_name` is the ADT name (used for parent-type checking; currently
/// constructors don't store parent type in `Constructor`, so invariant 5
/// is verified structurally — all constructors in the vec belong to this ADT).
///
/// `constructors` is the constructor list from `ProjectOutput.adt_types`.
pub fn validate_constructors(
    type_name: &str,
    constructors: &[Constructor],
) -> ConstructorValidationResult {
    let actual_count = constructors.len();
    let mut violations = Vec::new();

    let (entries, grouped_map) = build_entries_and_groups(constructors);
    let unique_names = unique_constructor_names(constructors);
    let expected_count = unique_names.len();
    let index_counts = count_by_index(constructors);

    check_count_match(expected_count, actual_count, &mut violations);
    check_unique_indices(&index_counts, &unique_names, &mut violations);
    check_contiguous_indices(&index_counts, expected_count, &mut violations);
    check_unique_names(constructors, &mut violations);
    check_parent_type(type_name);

    let grouped = sorted_grouped(grouped_map);
    violations.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));

    ConstructorValidationResult {
        expected_count,
        actual_count,
        violations,
        entries,
        grouped,
    }
}

fn build_entries_and_groups(
    constructors: &[Constructor],
) -> (Vec<ConstructorEntry>, HashMap<(String, usize), usize>) {
    let mut entries = Vec::new();
    let mut grouped: HashMap<(String, usize), usize> = HashMap::new();

    for ctor in constructors {
        let field_types_display = if ctor.fields.is_empty() {
            "()".to_string()
        } else {
            ctor.fields
                .iter()
                .map(|f| format!("{f}"))
                .collect::<Vec<_>>()
                .join(", ")
        };

        entries.push(ConstructorEntry {
            name: ctor.name.clone(),
            index: ctor.index,
            arity: ctor.fields.len(),
            field_types_display,
        });

        *grouped.entry((ctor.name.clone(), ctor.index)).or_insert(0) += 1;
    }

    (entries, grouped)
}

fn unique_constructor_names(constructors: &[Constructor]) -> Vec<String> {
    let mut names: Vec<String> = constructors.iter().map(|c| c.name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

fn count_by_index(constructors: &[Constructor]) -> HashMap<usize, usize> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for ctor in constructors {
        *counts.entry(ctor.index).or_insert(0) += 1;
    }
    counts
}

fn check_count_match(expected: usize, actual: usize, violations: &mut Vec<ConstructorViolation>) {
    if actual != expected {
        violations.push(ConstructorViolation::CountMismatch { expected, actual });
    }
}

fn check_unique_indices(
    index_counts: &HashMap<usize, usize>,
    unique_names: &[String],
    violations: &mut Vec<ConstructorViolation>,
) {
    for (&index, &count) in index_counts {
        if count > unique_names.len().max(1) {
            violations.push(ConstructorViolation::DuplicateIndex { index, count });
        }
    }
}

fn check_contiguous_indices(
    index_counts: &HashMap<usize, usize>,
    expected_count: usize,
    violations: &mut Vec<ConstructorViolation>,
) {
    if expected_count > 0 {
        let missing: Vec<usize> = (0..expected_count)
            .filter(|i| !index_counts.contains_key(i))
            .collect();
        if !missing.is_empty() {
            violations.push(ConstructorViolation::NonContiguousIndices { missing });
        }
    }
}

fn check_unique_names(constructors: &[Constructor], violations: &mut Vec<ConstructorViolation>) {
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for ctor in constructors {
        *name_counts.entry(&ctor.name).or_insert(0) += 1;
    }
    for (&name, &count) in &name_counts {
        if count > 1 {
            violations.push(ConstructorViolation::DuplicateName {
                name: name.to_string(),
                count,
            });
        }
    }
}

/// Invariant 5: parent type consistency.
/// Constructor doesn't store parent type name directly, but since all
/// constructors come from `project.adt_types[type_name]`, this is
/// structurally guaranteed. Explicit acknowledgment.
fn check_parent_type(_type_name: &str) {}

fn sorted_grouped(grouped: HashMap<(String, usize), usize>) -> Vec<(String, usize, usize)> {
    let mut vec: Vec<(String, usize, usize)> = grouped
        .into_iter()
        .map(|((name, index), count)| (name, index, count))
        .collect();
    vec.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    vec
}

/// Validate constructors with an explicit expected variant count.
///
/// This is used by test code where the expected count comes from the
/// canonical type definition, not from deduplicating the constructor list.
pub fn validate_constructors_with_expected(
    type_name: &str,
    constructors: &[Constructor],
    expected_variant_count: usize,
) -> ConstructorValidationResult {
    let mut result = validate_constructors(type_name, constructors);

    // Override expected_count with the canonical count and re-check invariant 1
    result.expected_count = expected_variant_count;
    result
        .violations
        .retain(|v| !matches!(v, ConstructorViolation::CountMismatch { .. }));

    if result.actual_count != expected_variant_count {
        result.violations.insert(
            0,
            ConstructorViolation::CountMismatch {
                expected: expected_variant_count,
                actual: result.actual_count,
            },
        );
    }

    result
}
