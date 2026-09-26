//! Full-output cache coverage for `value_import_targets` (ADR 12.7.26a D7).
//!
//! Split from `full_output_tests.rs` for the file-size convention.

use super::*;
use crate::cache::build::BuildCache;
use crate::elaborate::{CoreDef, ImportTarget, ModuleExports, TypeProvenance, ValueImportTargets};

fn dummy_core_def(name: &str) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty: tungsten_core::Type::Nat,
        term: tungsten_core::SpannedTerm {
            term: tungsten_core::Term::Var(name.to_string()),
            span: None,
        },
        span: crate::span::Span::default(),
    }
}

fn entry_with_targets(targets: ValueImportTargets) -> CachedModuleFullOutput {
    CachedModuleFullOutput {
        defs: vec![dummy_core_def("f")],
        record_types: std::collections::HashMap::new(),
        adt_types: std::collections::HashMap::new(),
        type_aliases: std::collections::HashMap::new(),
        type_provenance: TypeProvenance::default(),
        encoded_types: std::collections::HashMap::new(),
        mutual_recursion_groups: std::collections::HashMap::new(),
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
        termination_meta: std::collections::HashMap::new(),
        value_import_targets: targets,
    }
}

/// Schema v2 pins the `value_import_targets` addition: old v1 entries key
/// differently and are cleanly invalidated.
#[test]
fn schema_version_is_bumped_for_value_import_targets() {
    assert!(
        FULL_OUTPUT_SCHEMA_VERSION >= 2,
        "ADR 12.7.26a D7 bumped the full-output schema to 2"
    );
}

#[test]
fn value_import_targets_roundtrip_through_cache() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let key = [0xDD; 32];

    let mut targets = ValueImportTargets::new();
    targets.insert(
        "describe".to_string(),
        ImportTarget::Unambiguous(vec!["a".to_string()]),
    );
    targets.insert(
        "pick".to_string(),
        ImportTarget::Ambiguous(vec![vec!["a".to_string()], vec!["b".to_string()]]),
    );
    let entry = entry_with_targets(targets.clone());

    cache.put_module_full_output(&key, &entry).unwrap();
    let loaded = cache
        .get_module_full_output(&key)
        .expect("should hit after put");
    assert_eq!(loaded.value_import_targets, targets);

    // ...and the table survives into the reconstructed ElabOutput, so a
    // full-output cache hit still feeds codegen's collision resolution.
    let output = loaded.into_elab_output();
    assert_eq!(output.value_import_targets, targets);
}
