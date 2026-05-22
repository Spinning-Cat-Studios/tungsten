use super::*;
use crate::cache::build::BuildCache;
use crate::elaborate::ModuleExports;
// --- Cache key tests ---

#[test]
fn test_compute_module_cache_key_deterministic() {
    let content_hash = [0xAA; 32];
    let exports_hash = [0xBB; 32];
    let key1 = compute_module_cache_key("0.1.0", &content_hash, &exports_hash);
    let key2 = compute_module_cache_key("0.1.0", &content_hash, &exports_hash);
    assert_eq!(key1, key2);
}

#[test]
fn test_cache_key_changes_with_compiler_version() {
    let content_hash = [0xAA; 32];
    let exports_hash = [0xBB; 32];
    let key1 = compute_module_cache_key("0.1.0", &content_hash, &exports_hash);
    let key2 = compute_module_cache_key("0.2.0", &content_hash, &exports_hash);
    assert_ne!(key1, key2);
}

#[test]
fn test_cache_key_changes_with_content() {
    let exports_hash = [0xBB; 32];
    let key1 = compute_module_cache_key("0.1.0", &[0xAA; 32], &exports_hash);
    let key2 = compute_module_cache_key("0.1.0", &[0xCC; 32], &exports_hash);
    assert_ne!(key1, key2);
}

#[test]
fn test_cache_key_changes_with_exports() {
    let content_hash = [0xAA; 32];
    let key1 = compute_module_cache_key("0.1.0", &content_hash, &[0xBB; 32]);
    let key2 = compute_module_cache_key("0.1.0", &content_hash, &[0xCC; 32]);
    assert_ne!(key1, key2);
}

// --- Hash tests ---

#[test]
fn test_hash_exports_deterministic() {
    let exports = ModuleExports::default();
    let h1 = hash_exports(&exports);
    let h2 = hash_exports(&exports);
    assert_eq!(h1, h2);
}

#[test]
fn test_hash_file_content_deterministic() {
    let h1 = hash_file_content(b"fn main() -> Nat { 42 }");
    let h2 = hash_file_content(b"fn main() -> Nat { 42 }");
    assert_eq!(h1, h2);
}

#[test]
fn test_hash_file_content_different() {
    let h1 = hash_file_content(b"fn main() -> Nat { 42 }");
    let h2 = hash_file_content(b"fn main() -> Nat { 43 }");
    assert_ne!(h1, h2);
}

#[test]
fn test_hash_exports_changes_with_new_type() {
    let empty = ModuleExports::default();
    let mut with_type = ModuleExports::default();
    with_type.types.push((
        "Foo".to_string(),
        crate::elaborate::TypeDef {
            name: "Foo".to_string(),
            params: Vec::new(),
            kind: crate::elaborate::TypeDefKind::Stub,
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::default(),
            defining_module: None,
            encoded_type: None,
            field_visibilities: Vec::new(),
        },
    ));
    assert_ne!(hash_exports(&empty), hash_exports(&with_type));
}

#[test]
fn test_hash_exports_changes_with_new_constructor() {
    let empty = ModuleExports::default();
    let mut with_ctor = ModuleExports::default();
    with_ctor.constructors.push((
        "Nil".to_string(),
        crate::elaborate::ConstructorInfo::test_stub("List", 0, 0),
    ));
    assert_ne!(hash_exports(&empty), hash_exports(&with_ctor));
}

#[test]
fn test_hash_exports_changes_with_kind_change() {
    let mut adt_exports = ModuleExports::default();
    adt_exports.types.push((
        "Foo".to_string(),
        crate::elaborate::TypeDef {
            name: "Foo".to_string(),
            params: Vec::new(),
            kind: crate::elaborate::TypeDefKind::ADT(Vec::new()),
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::default(),
            defining_module: None,
            encoded_type: None,
            field_visibilities: Vec::new(),
        },
    ));
    let mut record_exports = ModuleExports::default();
    record_exports.types.push((
        "Foo".to_string(),
        crate::elaborate::TypeDef {
            name: "Foo".to_string(),
            params: Vec::new(),
            kind: crate::elaborate::TypeDefKind::Record(Vec::new()),
            visibility: crate::ast::Visibility::Public,
            span: crate::span::Span::default(),
            defining_module: None,
            encoded_type: None,
            field_visibilities: Vec::new(),
        },
    ));
    assert_ne!(hash_exports(&adt_exports), hash_exports(&record_exports));
}

// --- Signature serialization tests ---

#[test]
fn test_cached_module_signature_roundtrip() {
    let sig = CachedModuleSignature {
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
        def_count: 0,
    };

    let bytes = bincode::serialize(&sig).unwrap();
    let deserialized: CachedModuleSignature = bincode::deserialize(&bytes).unwrap();
    assert_eq!(deserialized.delta_exports.types.len(), 0);
    assert!(deserialized.warnings.is_empty());
}

// --- BuildCache integration tests ---

#[test]
fn test_get_module_elab_miss_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let key = [0xAA; 32];
    assert!(cache.get_module_elab(&key).is_none());
}

#[test]
fn test_put_then_get_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let key = [0xBB; 32];
    let sig = empty_signature();
    cache.put_module_elab(&key, &sig).unwrap();
    let loaded = cache.get_module_elab(&key).expect("should hit after put");
    assert_eq!(loaded.delta_exports.types.len(), 0);
}

#[test]
fn test_clean_elab_cache_removes_entries() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let key = [0xCC; 32];
    let sig = empty_signature();
    cache.put_module_elab(&key, &sig).unwrap();
    assert!(cache.get_module_elab(&key).is_some());
    cache.clean_elab_cache().unwrap();
    assert!(cache.get_module_elab(&key).is_none());
}

#[test]
fn test_elab_cache_stats_counts() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();

    let stats = cache.elab_cache_stats().unwrap();
    assert_eq!(stats.entry_count, 0);
    assert_eq!(stats.size_bytes, 0);

    let sig = empty_signature();
    cache.put_module_elab(&[0x01; 32], &sig).unwrap();
    cache.put_module_elab(&[0x02; 32], &sig).unwrap();

    let stats = cache.elab_cache_stats().unwrap();
    assert_eq!(stats.entry_count, 2);
    assert!(stats.size_bytes > 0);
}

// --- Delta export tests (ADR 10.5.26n) ---

#[test]
fn test_compute_delta_exports_filters_prior() {
    let mut prior = ModuleExports::default();
    prior
        .values
        .push(("existing_fn".to_string(), stub_value_def()));
    prior.constructors.push((
        "OldCtor".to_string(),
        crate::elaborate::ConstructorInfo {
            type_name: "OldType".to_string(),
            index: 0,
            arity: 0,
            visibility: None,
            defining_module: None,
        },
    ));

    let mut full = prior.clone();
    full.values.push(("new_fn".to_string(), stub_value_def()));
    full.constructors.push((
        "NewCtor".to_string(),
        crate::elaborate::ConstructorInfo {
            type_name: "NewType".to_string(),
            index: 0,
            arity: 1,
            visibility: None,
            defining_module: None,
        },
    ));

    let delta = compute_delta_exports(&full, &prior);
    assert_eq!(delta.values.len(), 1);
    assert_eq!(delta.values[0].0, "new_fn");
    assert_eq!(delta.constructors.len(), 1);
    assert_eq!(delta.constructors[0].0, "NewCtor");
    assert_eq!(delta.types.len(), 0);
}

#[test]
fn test_compute_delta_exports_empty_prior_returns_all() {
    let prior = ModuleExports::default();
    let mut full = ModuleExports::default();
    full.values.push(("a".to_string(), stub_value_def()));
    full.values.push(("b".to_string(), stub_value_def()));

    let delta = compute_delta_exports(&full, &prior);
    assert_eq!(delta.values.len(), 2);
}

#[test]
fn test_compute_delta_exports_identical_returns_empty() {
    let mut exports = ModuleExports::default();
    exports.values.push(("f".to_string(), stub_value_def()));
    exports.types.push(("T".to_string(), stub_type_def("T")));

    let delta = compute_delta_exports(&exports, &exports);
    assert_eq!(delta.values.len(), 0);
    assert_eq!(delta.types.len(), 0);
    assert_eq!(delta.constructors.len(), 0);
}

#[test]
fn test_into_elab_output_has_empty_defs_and_type_maps() {
    let sig = CachedModuleSignature {
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
        def_count: 42,
    };
    let output = sig.into_elab_output();
    assert!(output.defs.is_empty());
    assert!(output.record_types.is_empty());
    assert!(output.adt_types.is_empty());
    assert!(output.encoded_types.is_empty());
}

#[test]
fn test_from_output_preserves_def_count() {
    use std::collections::HashMap;
    let dummy_term = tungsten_core::SpannedTerm {
        term: tungsten_core::Term::Var("x".to_string()),
        span: None,
    };
    let output = crate::elaborate::ElabOutput {
        defs: vec![
            crate::elaborate::CoreDef {
                name: "f".to_string(),
                term: dummy_term.clone(),
                ty: tungsten_core::Type::Nat,
                span: crate::span::Span::default(),
            },
            crate::elaborate::CoreDef {
                name: "g".to_string(),
                term: dummy_term,
                ty: tungsten_core::Type::Nat,
                span: crate::span::Span::default(),
            },
        ],
        warnings: Vec::new(),
        record_types: HashMap::new(),
        adt_types: HashMap::new(),
        type_aliases: HashMap::new(),
        type_provenance: crate::elaborate::TypeProvenance::default(),
        encoded_types: HashMap::new(),
        mutual_recursion_groups: HashMap::new(),
        type_visibilities: HashMap::new(),
        record_field_visibilities: HashMap::new(),
    };
    let exports = ModuleExports::default();
    let sig = CachedModuleSignature::from_output(&output, &exports, &exports);
    assert_eq!(sig.def_count, 2);
}

#[test]
fn test_hash_exports_value_name_sensitivity() {
    let empty = ModuleExports::default();
    let mut with_value = ModuleExports::default();
    with_value
        .values
        .push(("my_fn".to_string(), stub_value_def()));
    assert_ne!(hash_exports(&empty), hash_exports(&with_value));
}

#[test]
fn test_cache_hit_then_miss_after_content_change() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let exports_hash = [0xDD; 32];

    let content_a = hash_file_content(b"fn f() -> Nat { 1 }");
    let key_a = compute_module_cache_key("0.1.0", &content_a, &exports_hash);
    let sig = empty_signature();
    cache.put_module_elab(&key_a, &sig).unwrap();
    assert!(cache.get_module_elab(&key_a).is_some());

    // Different content produces a different key → miss
    let content_b = hash_file_content(b"fn f() -> Nat { 2 }");
    let key_b = compute_module_cache_key("0.1.0", &content_b, &exports_hash);
    assert!(cache.get_module_elab(&key_b).is_none());
}

// --- Helpers ---

fn empty_signature() -> CachedModuleSignature {
    CachedModuleSignature {
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
        def_count: 0,
    }
}

fn stub_value_def() -> crate::elaborate::ValueDef {
    crate::elaborate::ValueDef {
        name: String::new(),
        ty: tungsten_core::Type::Nat,
        visibility: crate::ast::Visibility::Private,
        span: crate::span::Span::default(),
    }
}

fn stub_type_def(name: &str) -> crate::elaborate::TypeDef {
    crate::elaborate::TypeDef {
        name: name.to_string(),
        params: Vec::new(),
        kind: crate::elaborate::TypeDefKind::Stub,
        visibility: crate::ast::Visibility::Public,
        span: crate::span::Span::default(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    }
}
