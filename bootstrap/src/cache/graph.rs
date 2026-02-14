//! Module dependency graph for incremental compilation.
//!
//! Tracks which modules depend on which, enabling cascade invalidation
//! when a module changes.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A module in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    /// Canonical path to this module's source file.
    pub path: PathBuf,
    /// Content hash of the source file.
    pub content_hash: [u8; 32],
    /// Modules this module directly depends on (via `mod foo;`).
    pub dependencies: Vec<PathBuf>,
    /// Modules that depend on this module (reverse edges).
    pub dependents: Vec<PathBuf>,
}

/// The full dependency graph for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// Module nodes indexed by canonical path.
    pub modules: HashMap<PathBuf, ModuleNode>,
    /// Root module (entry point).
    pub root: Option<PathBuf>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a module to the graph.
    ///
    /// Note: Reverse edges (dependents) are computed lazily when needed,
    /// since modules may be added in any order.
    pub fn add_module(
        &mut self,
        path: PathBuf,
        content_hash: [u8; 32],
        dependencies: Vec<PathBuf>,
    ) {
        self.modules.insert(
            path.clone(),
            ModuleNode {
                path,
                content_hash,
                dependencies,
                dependents: Vec::new(),
            },
        );
    }

    /// Rebuild all reverse edges (dependents) from forward edges (dependencies).
    ///
    /// Call this after adding all modules to ensure dependents are correct.
    pub fn rebuild_reverse_edges(&mut self) {
        // Clear all dependents first.
        for node in self.modules.values_mut() {
            node.dependents.clear();
        }

        // Collect all path -> dependencies mappings.
        let deps: Vec<(PathBuf, Vec<PathBuf>)> = self
            .modules
            .values()
            .map(|n| (n.path.clone(), n.dependencies.clone()))
            .collect();

        // Rebuild reverse edges.
        for (path, dependencies) in deps {
            for dep in dependencies {
                if let Some(dep_node) = self.modules.get_mut(&dep) {
                    if !dep_node.dependents.contains(&path) {
                        dep_node.dependents.push(path.clone());
                    }
                }
            }
        }
    }

    /// Set the root module.
    pub fn set_root(&mut self, root: PathBuf) {
        self.root = Some(root);
    }

    /// Get all modules that transitively depend on the given module.
    pub fn transitive_dependents(&self, module: &Path) -> HashSet<PathBuf> {
        let mut result = HashSet::new();
        let mut worklist = vec![module.to_path_buf()];

        while let Some(current) = worklist.pop() {
            if let Some(node) = self.modules.get(&current) {
                for dep in &node.dependents {
                    if result.insert(dep.clone()) {
                        worklist.push(dep.clone());
                    }
                }
            }
        }
        result
    }

    /// Compute the set of modules that need to be re-processed.
    ///
    /// This includes:
    /// 1. Modules whose content hash has changed
    /// 2. All transitive dependents of changed modules
    pub fn compute_invalidation(
        &self,
        current_hashes: &HashMap<PathBuf, [u8; 32]>,
    ) -> HashSet<PathBuf> {
        let mut invalid = HashSet::new();

        // Phase 1: Find directly changed modules.
        for (path, node) in &self.modules {
            match current_hashes.get(path) {
                Some(hash) if *hash != node.content_hash => {
                    // Content changed.
                    invalid.insert(path.clone());
                }
                None => {
                    // Module no longer exists or not in current build.
                    invalid.insert(path.clone());
                }
                _ => {}
            }
        }

        // Check for new modules not in graph.
        for path in current_hashes.keys() {
            if !self.modules.contains_key(path) {
                invalid.insert(path.clone());
            }
        }

        // Phase 2: Flood-fill to all transitive dependents.
        let mut worklist: Vec<_> = invalid.iter().cloned().collect();
        while let Some(changed) = worklist.pop() {
            if let Some(node) = self.modules.get(&changed) {
                for dependent in &node.dependents {
                    if invalid.insert(dependent.clone()) {
                        worklist.push(dependent.clone());
                    }
                }
            }
        }

        invalid
    }
}
