//! Tests for dependency graph operations.

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
