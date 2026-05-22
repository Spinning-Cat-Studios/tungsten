//! Tests for IR cache operations and types hash computation.

use crate::ast::*;
use crate::cache::*;
use crate::span::Span;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_temp_project() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    (temp, root)
}

fn create_test_source(root: &PathBuf, name: &str, content: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, content).unwrap();
    path
}

fn create_minimal_ast() -> SourceFile {
    SourceFile {
        items: vec![],
        span: Span::default(),
    }
}

#[test]
fn test_ir_cache_miss_without_ast() {
    let (_temp, root) = create_temp_project();
    let mut cache = BuildCache::new(&root, false).unwrap();

    let source_path = create_test_source(&root, "test.tg", "mod test");
    let types_hash = [42u8; 32];

    // Should return None since there's no AST cached
    assert!(cache.get_ir(&source_path, &types_hash).is_none());
}

#[test]
fn test_ir_cache_requires_ast_first() {
    let (_temp, root) = create_temp_project();
    let mut cache = BuildCache::new(&root, false).unwrap();

    let source_path = create_test_source(&root, "test.tg", "mod test");
    let types_hash = [42u8; 32];
    let defs: Vec<crate::elaborate::CoreDef> = vec![];

    // Should fail since there's no AST entry
    let result = cache.put_ir(&source_path, types_hash, &defs);
    assert!(result.is_err());
}

#[test]
fn test_ir_cache_hit() {
    let (_temp, root) = create_temp_project();
    let mut cache = BuildCache::new(&root, false).unwrap();

    let source_path = create_test_source(&root, "test.tg", "mod test");
    let content = "mod test";
    let ast = create_minimal_ast();
    let types_hash = [42u8; 32];
    let defs: Vec<crate::elaborate::CoreDef> = vec![];

    // Put AST first, then IR
    cache.put(&source_path, content, &ast).unwrap();
    cache.put_ir(&source_path, types_hash, &defs).unwrap();

    // Should hit
    let cached_ir = cache.get_ir(&source_path, &types_hash);
    assert!(cached_ir.is_some());
}

#[test]
fn test_ir_cache_miss_on_types_change() {
    let (_temp, root) = create_temp_project();
    let mut cache = BuildCache::new(&root, false).unwrap();

    let source_path = create_test_source(&root, "test.tg", "mod test");
    let content = "mod test";
    let ast = create_minimal_ast();
    let types_hash = [42u8; 32];
    let defs: Vec<crate::elaborate::CoreDef> = vec![];

    // Put AST and IR
    cache.put(&source_path, content, &ast).unwrap();
    cache.put_ir(&source_path, types_hash, &defs).unwrap();

    // Should miss with different types hash
    let different_hash = [99u8; 32];
    assert!(cache.get_ir(&source_path, &different_hash).is_none());
}

#[test]
fn test_ir_cache_persistence() {
    let (_temp, root) = create_temp_project();
    let source_path = create_test_source(&root, "test.tg", "mod test");
    let content = "mod test";
    let ast = create_minimal_ast();
    let types_hash = [42u8; 32];
    let defs: Vec<crate::elaborate::CoreDef> = vec![];

    // Store in one cache instance
    {
        let mut cache = BuildCache::new(&root, false).unwrap();
        cache.put(&source_path, content, &ast).unwrap();
        cache.put_ir(&source_path, types_hash, &defs).unwrap();
    }

    // Load in new instance
    {
        let mut cache = BuildCache::new(&root, false).unwrap();
        let cached_ir = cache.get_ir(&source_path, &types_hash);
        assert!(cached_ir.is_some());
    }
}

#[test]
fn test_compute_types_hash_deterministic() {
    use crate::ast::Visibility;
    use crate::elaborate::{TypeDef, TypeDefKind};
    use crate::span::Span;
    use tungsten_core::Type;

    let types: Vec<(String, TypeDef)> = vec![
        (
            "Foo".to_string(),
            TypeDef {
                name: "Foo".to_string(),
                params: vec![],
                kind: TypeDefKind::Record(vec![("x".to_string(), Type::Nat)]),
                visibility: Visibility::Private,
                span: Span::default(),
                defining_module: None,
                encoded_type: None,
                field_visibilities: Vec::new(),
            },
        ),
        (
            "Bar".to_string(),
            TypeDef {
                name: "Bar".to_string(),
                params: vec![],
                kind: TypeDefKind::Record(vec![("y".to_string(), Type::Bool)]),
                visibility: Visibility::Private,
                span: Span::default(),
                defining_module: None,
                encoded_type: None,
                field_visibilities: Vec::new(),
            },
        ),
    ];

    let hash1 = BuildCache::compute_types_hash(&types);
    let hash2 = BuildCache::compute_types_hash(&types);
    assert_eq!(hash1, hash2);
}

#[test]
fn test_compute_types_hash_order_independent() {
    use crate::ast::Visibility;
    use crate::elaborate::{TypeDef, TypeDefKind};
    use crate::span::Span;
    use tungsten_core::Type;

    let types1: Vec<(String, TypeDef)> = vec![
        (
            "Foo".to_string(),
            TypeDef {
                name: "Foo".to_string(),
                params: vec![],
                kind: TypeDefKind::Record(vec![("x".to_string(), Type::Nat)]),
                visibility: Visibility::Private,
                span: Span::default(),
                defining_module: None,
                encoded_type: None,
                field_visibilities: Vec::new(),
            },
        ),
        (
            "Bar".to_string(),
            TypeDef {
                name: "Bar".to_string(),
                params: vec![],
                kind: TypeDefKind::Record(vec![("y".to_string(), Type::Bool)]),
                visibility: Visibility::Private,
                span: Span::default(),
                defining_module: None,
                encoded_type: None,
                field_visibilities: Vec::new(),
            },
        ),
    ];

    let types2: Vec<(String, TypeDef)> = vec![
        (
            "Bar".to_string(),
            TypeDef {
                name: "Bar".to_string(),
                params: vec![],
                kind: TypeDefKind::Record(vec![("y".to_string(), Type::Bool)]),
                visibility: Visibility::Private,
                span: Span::default(),
                defining_module: None,
                encoded_type: None,
                field_visibilities: Vec::new(),
            },
        ),
        (
            "Foo".to_string(),
            TypeDef {
                name: "Foo".to_string(),
                params: vec![],
                kind: TypeDefKind::Record(vec![("x".to_string(), Type::Nat)]),
                visibility: Visibility::Private,
                span: Span::default(),
                defining_module: None,
                encoded_type: None,
                field_visibilities: Vec::new(),
            },
        ),
    ];

    // Should be the same since we sort by name internally
    let hash1 = BuildCache::compute_types_hash(&types1);
    let hash2 = BuildCache::compute_types_hash(&types2);
    assert_eq!(hash1, hash2);
}

#[test]
fn test_compute_types_hash_different_for_different_types() {
    use crate::ast::Visibility;
    use crate::elaborate::{TypeDef, TypeDefKind};
    use crate::span::Span;
    use tungsten_core::Type;

    let types1: Vec<(String, TypeDef)> = vec![(
        "Foo".to_string(),
        TypeDef {
            name: "Foo".to_string(),
            params: vec![],
            kind: TypeDefKind::Record(vec![("x".to_string(), Type::Nat)]),
            visibility: Visibility::Private,
            span: Span::default(),
            defining_module: None,
            encoded_type: None,
            field_visibilities: Vec::new(),
        },
    )];

    let types2: Vec<(String, TypeDef)> = vec![(
        "Foo".to_string(),
        TypeDef {
            name: "Foo".to_string(),
            params: vec![],
            kind: TypeDefKind::Record(vec![("x".to_string(), Type::Bool)]), // Different type
            visibility: Visibility::Private,
            span: Span::default(),
            defining_module: None,
            encoded_type: None,
            field_visibilities: Vec::new(),
        },
    )];

    let hash1 = BuildCache::compute_types_hash(&types1);
    let hash2 = BuildCache::compute_types_hash(&types2);
    assert_ne!(hash1, hash2);
}
