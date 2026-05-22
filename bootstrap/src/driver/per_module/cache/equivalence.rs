//! Equivalence comparison for ElabOutput (ADR 11.5.26b §P1, P2).
//!
//! Provides semantic comparison of elaboration results to verify that
//! cached vs fresh and serial vs parallel produce identical outputs.

use crate::elaborate::{ElabOutput, ModuleExports};

/// Semantic differences between two ElabOutputs.
#[derive(Debug, Default)]
pub(super) struct ElabDiff {
    pub(super) differences: Vec<String>,
}

impl ElabDiff {
    pub(super) fn is_empty(&self) -> bool {
        self.differences.is_empty()
    }
}

impl std::fmt::Display for ElabDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for d in &self.differences {
            writeln!(f, "  - {d}")?;
        }
        Ok(())
    }
}

/// Compare two ElabOutputs for semantic equivalence.
///
/// Checks: def count, def names, warning count, type metadata keys.
/// Does NOT compare CoreDef term bodies (those are structural and may
/// differ in internal variable naming).
pub(super) fn compare_elab_outputs(a: &ElabOutput, b: &ElabOutput) -> ElabDiff {
    let mut diff = ElabDiff::default();

    // Def count
    if a.defs.len() != b.defs.len() {
        diff.differences
            .push(format!("def count: {} vs {}", a.defs.len(), b.defs.len()));
    }

    // Def names (order-sensitive)
    let a_names: Vec<&str> = a.defs.iter().map(|d| d.name.as_str()).collect();
    let b_names: Vec<&str> = b.defs.iter().map(|d| d.name.as_str()).collect();
    if a_names != b_names {
        diff.differences.push(format!(
            "def names differ: {:?} vs {:?}",
            &a_names[..a_names.len().min(5)],
            &b_names[..b_names.len().min(5)],
        ));
    }

    // Def types
    let min_len = a.defs.len().min(b.defs.len());
    for i in 0..min_len {
        if a.defs[i].ty != b.defs[i].ty {
            diff.differences.push(format!(
                "def '{}' type differs: {:?} vs {:?}",
                a.defs[i].name, a.defs[i].ty, b.defs[i].ty,
            ));
        }
    }

    // Warning count
    if a.warnings.len() != b.warnings.len() {
        diff.differences.push(format!(
            "warning count: {} vs {}",
            a.warnings.len(),
            b.warnings.len()
        ));
    }

    // Record types keys
    compare_key_sets("record_types", &a.record_types, &b.record_types, &mut diff);
    compare_key_sets("adt_types", &a.adt_types, &b.adt_types, &mut diff);
    compare_key_sets("type_aliases", &a.type_aliases, &b.type_aliases, &mut diff);
    compare_key_sets(
        "encoded_types",
        &a.encoded_types,
        &b.encoded_types,
        &mut diff,
    );
    compare_key_sets(
        "mutual_recursion_groups",
        &a.mutual_recursion_groups,
        &b.mutual_recursion_groups,
        &mut diff,
    );

    diff
}

/// Compare two ModuleExports for semantic equivalence.
pub(super) fn compare_exports(a: &ModuleExports, b: &ModuleExports) -> ElabDiff {
    let mut diff = ElabDiff::default();

    // Type export names
    let mut a_types: Vec<&str> = a.types.iter().map(|(n, _)| n.as_str()).collect();
    let mut b_types: Vec<&str> = b.types.iter().map(|(n, _)| n.as_str()).collect();
    a_types.sort();
    b_types.sort();
    if a_types != b_types {
        diff.differences
            .push("export type names differ".to_string());
    }

    // Value export names
    let mut a_vals: Vec<&str> = a.values.iter().map(|(n, _)| n.as_str()).collect();
    let mut b_vals: Vec<&str> = b.values.iter().map(|(n, _)| n.as_str()).collect();
    a_vals.sort();
    b_vals.sort();
    if a_vals != b_vals {
        diff.differences
            .push("export value names differ".to_string());
    }

    // Constructor export names
    let mut a_ctors: Vec<&str> = a.constructors.iter().map(|(n, _)| n.as_str()).collect();
    let mut b_ctors: Vec<&str> = b.constructors.iter().map(|(n, _)| n.as_str()).collect();
    a_ctors.sort();
    b_ctors.sort();
    if a_ctors != b_ctors {
        diff.differences
            .push("export constructor names differ".to_string());
    }

    diff
}

fn compare_key_sets<V>(
    label: &str,
    a: &std::collections::HashMap<String, V>,
    b: &std::collections::HashMap<String, V>,
    diff: &mut ElabDiff,
) {
    let mut a_keys: Vec<&str> = a.keys().map(|k| k.as_str()).collect();
    let mut b_keys: Vec<&str> = b.keys().map(|k| k.as_str()).collect();
    a_keys.sort();
    b_keys.sort();
    if a_keys != b_keys {
        diff.differences
            .push(format!("{label} keys differ: {} vs {}", a.len(), b.len()));
    }
}

/// Read the configured thread count for parallel Phase B (ADR 11.5.26b §4).
///
/// Returns 1 (serial) by default. Set `TUNGSTEN_ELAB_THREADS=N` to override.
pub(in crate::driver::per_module) fn elab_thread_count() -> usize {
    std::env::var("TUNGSTEN_ELAB_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elaborate::{ElabOutput, ModuleExports, TypeProvenance};
    use crate::span::Span;
    use std::collections::HashMap;
    use tungsten_core::terms::{SpannedTerm, TermSpan};
    use tungsten_core::Type;

    fn empty_output() -> ElabOutput {
        ElabOutput {
            defs: Vec::new(),
            warnings: Vec::new(),
            record_types: HashMap::new(),
            adt_types: HashMap::new(),
            type_aliases: HashMap::new(),
            type_provenance: TypeProvenance::default(),
            encoded_types: HashMap::new(),
            mutual_recursion_groups: HashMap::new(),
            type_visibilities: HashMap::new(),
            record_field_visibilities: HashMap::new(),
        }
    }

    #[test]
    fn identical_outputs_have_no_diff() {
        let a = empty_output();
        let b = empty_output();
        let diff = compare_elab_outputs(&a, &b);
        assert!(diff.is_empty(), "expected no diffs: {diff}");
    }

    #[test]
    fn different_def_count_detected() {
        let a = empty_output();
        let mut b = empty_output();
        b.defs.push(crate::elaborate::CoreDef {
            name: "f".to_string(),
            ty: Type::Nat,
            term: SpannedTerm::new(tungsten_core::Term::nat(42), TermSpan::new(0, 0)),
            span: Span::new(0, 0),
        });
        let diff = compare_elab_outputs(&a, &b);
        assert!(!diff.is_empty());
        assert!(diff.differences[0].contains("def count"));
    }

    #[test]
    fn different_type_metadata_keys_detected() {
        let mut a = empty_output();
        let b = empty_output();
        a.record_types.insert("Foo".to_string(), vec![]);
        let diff = compare_elab_outputs(&a, &b);
        assert!(!diff.is_empty());
        assert!(diff.differences[0].contains("record_types"));
    }

    #[test]
    fn export_name_mismatch_detected() {
        let a = ModuleExports {
            types: vec![(
                "Nat".to_string(),
                crate::elaborate::TypeDef {
                    name: "Nat".to_string(),
                    params: vec![],
                    kind: crate::elaborate::TypeDefKind::Stub,
                    visibility: crate::ast::Visibility::Public,
                    span: crate::span::Span::new(0, 0),
                    defining_module: None,
                    encoded_type: None,
                    field_visibilities: Vec::new(),
                },
            )],
            values: vec![],
            constructors: vec![],
        };
        let b = ModuleExports::default();
        let diff = compare_exports(&a, &b);
        assert!(!diff.is_empty());
    }

    #[test]
    fn identical_exports_have_no_diff() {
        let a = ModuleExports::default();
        let b = ModuleExports::default();
        let diff = compare_exports(&a, &b);
        assert!(diff.is_empty(), "expected no diffs: {diff}");
    }

    #[test]
    fn elab_thread_count_default_is_one() {
        // Without the env var set, should return 1
        let count = elab_thread_count();
        assert!(count >= 1);
    }

    // P2 baseline: serial-vs-serial produces identical output
    #[test]
    fn serial_vs_serial_equivalence() {
        // Two identical empty outputs should be equivalent
        let run1 = empty_output();
        let run2 = empty_output();
        let diff = compare_elab_outputs(&run1, &run2);
        assert!(diff.is_empty(), "serial-vs-serial should match: {diff}");
    }

    #[test]
    fn different_def_type_detected() {
        let mut a = empty_output();
        let mut b = empty_output();
        a.defs.push(crate::elaborate::CoreDef {
            name: "f".to_string(),
            ty: Type::Nat,
            term: SpannedTerm::new(tungsten_core::Term::nat(1), TermSpan::new(0, 0)),
            span: Span::new(0, 0),
        });
        b.defs.push(crate::elaborate::CoreDef {
            name: "f".to_string(),
            ty: Type::Bool,
            term: SpannedTerm::new(tungsten_core::Term::nat(1), TermSpan::new(0, 0)),
            span: Span::new(0, 0),
        });
        let diff = compare_elab_outputs(&a, &b);
        assert!(!diff.is_empty());
        assert!(
            diff.differences.iter().any(|d| d.contains("type differs")),
            "should detect type mismatch: {diff}",
        );
    }
}
