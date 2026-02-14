//! Build cache module.
//!
//! This module provides caching for parsed ASTs and elaborated IR to speed up
//! incremental compilation. It includes:
//!
//! - Content-based cache invalidation (SHA-256 hashes)
//! - Schema versioning for safe cache upgrades
//! - Dependency graphs for cascade invalidation
//! - LRU eviction when the cache exceeds its size limit
//!
//! # Usage
//!
//! ```ignore
//! use tungsten::cache::BuildCache;
//!
//! let mut cache = BuildCache::new(project_root, verbose)?;
//!
//! // Try to get cached AST
//! if let Some(ast) = cache.get(&path, &content) {
//!     // Cache hit - use cached AST
//! } else {
//!     // Cache miss - parse and cache
//!     let ast = parse(&content)?;
//!     cache.put(&path, &content, &ast)?;
//! }
//! ```

mod build;
mod graph;
mod schema;
mod types;

pub use build::BuildCache;
pub use graph::DependencyGraph;
pub use schema::{
    compute_schema_hash, AST_SCHEMA_HASH, AST_SCHEMA_SIGNATURE, CACHE_FORMAT_VERSION,
    COMPILER_VERSION, DEFAULT_MAX_SIZE_MB, IR_SCHEMA_VERSION,
};
pub use types::{CacheConfig, CacheEntry, CacheManifest, CacheStats, PruneStats};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
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
    fn test_cache_creation() {
        let (_temp, root) = create_temp_project();
        let cache = BuildCache::new(&root, false).unwrap();

        // Check directories were created.
        assert!(root.join(".tungsten").exists());
        assert!(root.join(".tungsten/cache").exists());
        assert!(root.join(".tungsten/cache/modules").exists());

        // Check manifest is empty.
        let stats = cache.stats().unwrap();
        assert_eq!(stats.entry_count, 0);
    }

    #[test]
    fn test_cache_miss_hit() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        let source_path = create_test_source(&root, "test.tg", "mod test");
        let content = "mod test";
        let ast = create_minimal_ast();

        // First access should be a miss.
        assert!(cache.get(&source_path, content).is_none());

        // Store in cache.
        cache.put(&source_path, content, &ast).unwrap();

        // Second access should be a hit.
        let cached = cache.get(&source_path, content);
        assert!(cached.is_some());
    }

    #[test]
    fn test_cache_invalidation_on_content_change() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        let source_path = create_test_source(&root, "test.tg", "mod test");
        let original_content = "mod test";
        let ast = create_minimal_ast();

        // Store original.
        cache.put(&source_path, original_content, &ast).unwrap();
        assert!(cache.get(&source_path, original_content).is_some());

        // Different content should miss (even without updating file).
        let new_content = "mod test2";
        assert!(cache.get(&source_path, new_content).is_none());
    }

    #[test]
    fn test_cache_persistence() {
        let (_temp, root) = create_temp_project();
        let source_path = create_test_source(&root, "test.tg", "mod test");
        let content = "mod test";
        let ast = create_minimal_ast();

        // Create cache and store.
        {
            let mut cache = BuildCache::new(&root, false).unwrap();
            cache.put(&source_path, content, &ast).unwrap();
        }

        // Load in new instance.
        {
            let mut cache = BuildCache::new(&root, false).unwrap();
            let cached = cache.get(&source_path, content);
            assert!(cached.is_some());
        }
    }

    #[test]
    fn test_cache_stats() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        let source1 = create_test_source(&root, "a.tg", "mod a");
        let source2 = create_test_source(&root, "b.tg", "mod b");

        cache.put(&source1, "mod a", &create_minimal_ast()).unwrap();
        cache.put(&source2, "mod b", &create_minimal_ast()).unwrap();

        let stats = cache.stats().unwrap();
        assert_eq!(stats.entry_count, 2);
        assert!(stats.size_bytes > 0);
    }

    #[test]
    fn test_cache_clear() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        let source = create_test_source(&root, "test.tg", "mod test");
        cache
            .put(&source, "mod test", &create_minimal_ast())
            .unwrap();

        assert!(cache.get(&source, "mod test").is_some());

        cache.clear().unwrap();

        assert!(cache.get(&source, "mod test").is_none());
        let stats = cache.stats().unwrap();
        assert_eq!(stats.entry_count, 0);
    }

    #[test]
    fn test_cache_prune() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        // Create many entries.
        for i in 0..10 {
            let name = format!("test{i}.tg");
            let content = format!("mod test{i}");
            let source = create_test_source(&root, &name, &content);
            cache.put(&source, &content, &create_minimal_ast()).unwrap();
        }

        let stats = cache.stats().unwrap();
        let initial_count = stats.entry_count;
        assert_eq!(initial_count, 10);

        // Prune to very small size to force eviction.
        let prune_stats = cache.prune(Some(0)).unwrap();
        assert!(prune_stats.removed_count > 0);
        assert!(prune_stats.freed_bytes > 0);
    }

    #[test]
    fn test_hash_consistency() {
        let content = b"mod test { fn foo() {} }";
        let hash1 = BuildCache::hash_content(content);
        let hash2 = BuildCache::hash_content(content);
        assert_eq!(hash1, hash2);

        let different = b"mod test { fn bar() {} }";
        let hash3 = BuildCache::hash_content(different);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_config_loading() {
        let (_temp, root) = create_temp_project();

        // Create custom config.
        let config_content = r#"
            max_size_mb = 500
        "#;
        fs::create_dir_all(root.join(".tungsten")).unwrap();
        fs::write(root.join(".tungsten/config.toml"), config_content).unwrap();

        let cache = BuildCache::new(&root, false).unwrap();
        let stats = cache.stats().unwrap();
        assert_eq!(stats.max_size_mb, 500);
    }

    #[test]
    fn test_multiple_files() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        let sources: Vec<_> = (0..5)
            .map(|i| {
                let name = format!("module{i}.tg");
                let content = format!("mod module{i}");
                let path = create_test_source(&root, &name, &content);
                (path, content, format!("module{i}"))
            })
            .collect();

        // Store all.
        for (path, content, _name) in &sources {
            cache.put(path, content, &create_minimal_ast()).unwrap();
        }

        // Verify all cached.
        for (path, content, _name) in &sources {
            let cached = cache.get(path, content);
            assert!(cached.is_some());
        }
    }

    #[test]
    fn test_subdirectory_files() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        // Create nested directory structure.
        let subdir = root.join("src/nested");
        fs::create_dir_all(&subdir).unwrap();

        let source_path = subdir.join("deep.tg");
        fs::write(&source_path, "mod deep").unwrap();

        let ast = create_minimal_ast();
        cache.put(&source_path, "mod deep", &ast).unwrap();

        let cached = cache.get(&source_path, "mod deep");
        assert!(cached.is_some());
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Schema Hash Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_schema_hash_is_deterministic() {
        // The hash should be computed at compile time and never change
        // during a single compilation
        let hash1 = AST_SCHEMA_HASH;
        let hash2 = AST_SCHEMA_HASH;
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_schema_hash_is_deterministic() {
        let signature = "Test{field:u32};Other{x:String};";
        let hash1 = compute_schema_hash(signature);
        let hash2 = compute_schema_hash(signature);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_schemas_different_hashes() {
        let sig1 = "Test{field:u32};";
        let sig2 = "Test{field:u64};"; // Changed type
        let sig3 = "Test{field:u32,extra:bool};"; // Added field

        let hash1 = compute_schema_hash(sig1);
        let hash2 = compute_schema_hash(sig2);
        let hash3 = compute_schema_hash(sig3);

        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash2, hash3);
    }

    #[test]
    fn test_schema_signature_is_not_empty() {
        assert!(!AST_SCHEMA_SIGNATURE.is_empty());
        // Should contain at least some struct definitions
        assert!(AST_SCHEMA_SIGNATURE.contains("SourceFile"));
    }

    #[test]
    fn test_cache_invalidated_on_schema_mismatch() {
        let (_temp, root) = create_temp_project();
        let source_path = create_test_source(&root, "test.tg", "mod test");
        let content = "mod test";
        let ast = create_minimal_ast();

        // Store with current schema
        {
            let mut cache = BuildCache::new(&root, false).unwrap();
            cache.put(&source_path, content, &ast).unwrap();
            assert!(cache.get(&source_path, content).is_some());
        }

        // Manually corrupt the manifest to have wrong schema hash
        {
            let manifest_path = root.join(".tungsten/cache/manifest.bin");
            let mut manifest: CacheManifest = {
                let file = fs::File::open(&manifest_path).unwrap();
                bincode::deserialize_from(std::io::BufReader::new(file)).unwrap()
            };
            // Change the schema hash to simulate a schema change
            manifest.ast_schema_hash = [0u8; 32];
            let file = fs::File::create(&manifest_path).unwrap();
            bincode::serialize_into(std::io::BufWriter::new(file), &manifest).unwrap();
        }

        // Loading cache should detect mismatch and clear
        {
            let mut cache = BuildCache::new(&root, true).unwrap();
            // Cache should be empty after schema mismatch detection
            assert!(cache.get(&source_path, content).is_none());
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Dependency Graph Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_dependency_graph_creation() {
        let graph = DependencyGraph::new();
        assert!(graph.modules.is_empty());
        assert!(graph.root.is_none());
    }

    #[test]
    fn test_dependency_graph_add_module() {
        let mut graph = DependencyGraph::new();
        let path = PathBuf::from("/test/module.tg");
        let hash = [1u8; 32];
        let deps = vec![PathBuf::from("/test/dep.tg")];

        graph.add_module(path.clone(), hash, deps.clone());

        assert!(graph.modules.contains_key(&path));
        let node = graph.modules.get(&path).unwrap();
        assert_eq!(node.content_hash, hash);
        assert_eq!(node.dependencies, deps);
    }

    #[test]
    fn test_dependency_graph_reverse_edges() {
        let mut graph = DependencyGraph::new();

        let a = PathBuf::from("/a.tg");
        let b = PathBuf::from("/b.tg");
        let c = PathBuf::from("/c.tg");

        // a depends on b, b depends on c
        graph.add_module(c.clone(), [3u8; 32], vec![]);
        graph.add_module(b.clone(), [2u8; 32], vec![c.clone()]);
        graph.add_module(a.clone(), [1u8; 32], vec![b.clone()]);

        graph.rebuild_reverse_edges();

        // c should have b as dependent
        assert!(graph.modules.get(&c).unwrap().dependents.contains(&b));
        // b should have a as dependent
        assert!(graph.modules.get(&b).unwrap().dependents.contains(&a));
        // a should have no dependents
        assert!(graph.modules.get(&a).unwrap().dependents.is_empty());
    }

    #[test]
    fn test_dependency_graph_transitive_dependents() {
        let mut graph = DependencyGraph::new();

        let a = PathBuf::from("/a.tg");
        let b = PathBuf::from("/b.tg");
        let c = PathBuf::from("/c.tg");
        let d = PathBuf::from("/d.tg");

        // d depends on c, c depends on b, b depends on a
        // If a changes, b, c, d should all be invalidated
        graph.add_module(a.clone(), [1u8; 32], vec![]);
        graph.add_module(b.clone(), [2u8; 32], vec![a.clone()]);
        graph.add_module(c.clone(), [3u8; 32], vec![b.clone()]);
        graph.add_module(d.clone(), [4u8; 32], vec![c.clone()]);

        graph.rebuild_reverse_edges();

        let dependents = graph.transitive_dependents(&a);
        assert!(dependents.contains(&b));
        assert!(dependents.contains(&c));
        assert!(dependents.contains(&d));
        assert!(!dependents.contains(&a)); // a itself not in dependents
    }

    #[test]
    fn test_dependency_graph_diamond() {
        let mut graph = DependencyGraph::new();

        // Diamond dependency: d depends on b and c, both depend on a
        //     a
        //    / \
        //   b   c
        //    \ /
        //     d
        let a = PathBuf::from("/a.tg");
        let b = PathBuf::from("/b.tg");
        let c = PathBuf::from("/c.tg");
        let d = PathBuf::from("/d.tg");

        graph.add_module(a.clone(), [1u8; 32], vec![]);
        graph.add_module(b.clone(), [2u8; 32], vec![a.clone()]);
        graph.add_module(c.clone(), [3u8; 32], vec![a.clone()]);
        graph.add_module(d.clone(), [4u8; 32], vec![b.clone(), c.clone()]);

        graph.rebuild_reverse_edges();

        // a has both b and c as dependents
        let a_node = graph.modules.get(&a).unwrap();
        assert!(a_node.dependents.contains(&b));
        assert!(a_node.dependents.contains(&c));

        // Transitive dependents of a should include b, c, d (not duplicated)
        let dependents = graph.transitive_dependents(&a);
        assert_eq!(dependents.len(), 3);
        assert!(dependents.contains(&b));
        assert!(dependents.contains(&c));
        assert!(dependents.contains(&d));
    }

    #[test]
    fn test_dependency_graph_compute_invalidation() {
        let mut graph = DependencyGraph::new();

        let a = PathBuf::from("/a.tg");
        let b = PathBuf::from("/b.tg");
        let c = PathBuf::from("/c.tg");

        let hash_a = [1u8; 32];
        let hash_b = [2u8; 32];
        let hash_c = [3u8; 32];

        graph.add_module(a.clone(), hash_a, vec![]);
        graph.add_module(b.clone(), hash_b, vec![a.clone()]);
        graph.add_module(c.clone(), hash_c, vec![b.clone()]);

        graph.rebuild_reverse_edges();

        // No changes - nothing invalidated
        let mut current_hashes = std::collections::HashMap::new();
        current_hashes.insert(a.clone(), hash_a);
        current_hashes.insert(b.clone(), hash_b);
        current_hashes.insert(c.clone(), hash_c);

        let invalid = graph.compute_invalidation(&current_hashes);
        assert!(invalid.is_empty());

        // Change a - should invalidate a, b, c
        let new_hash_a = [99u8; 32];
        current_hashes.insert(a.clone(), new_hash_a);

        let invalid = graph.compute_invalidation(&current_hashes);
        assert!(invalid.contains(&a));
        assert!(invalid.contains(&b));
        assert!(invalid.contains(&c));
    }

    #[test]
    fn test_cache_update_dependency_graph() {
        let (_temp, root) = create_temp_project();
        let mut cache = BuildCache::new(&root, false).unwrap();

        let a = root.join("a.tg");
        let b = root.join("b.tg");

        fs::write(&a, "mod a").unwrap();
        fs::write(&b, "mod b // imports a").unwrap();

        let modules = vec![
            (a.canonicalize().unwrap(), [1u8; 32], vec![]),
            (
                b.canonicalize().unwrap(),
                [2u8; 32],
                vec![a.canonicalize().unwrap()],
            ),
        ];

        cache.update_dependency_graph(root.canonicalize().unwrap(), modules);

        let graph = cache.dependency_graph();
        assert_eq!(graph.modules.len(), 2);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // IR Cache Tests
    // ─────────────────────────────────────────────────────────────────────────────

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

        // First put AST
        cache.put(&source_path, content, &ast).unwrap();

        // Then put IR
        cache.put_ir(&source_path, types_hash, &defs).unwrap();

        // Should get IR back
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
            },
        )];

        let hash1 = BuildCache::compute_types_hash(&types1);
        let hash2 = BuildCache::compute_types_hash(&types2);
        assert_ne!(hash1, hash2);
    }
}
