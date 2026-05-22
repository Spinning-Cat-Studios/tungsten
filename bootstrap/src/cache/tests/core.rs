//! Tests for the build cache module.

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

// ─────────────────────────────────────────────────────────────────────────────
// Build Cache Tests
// ─────────────────────────────────────────────────────────────────────────────

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
