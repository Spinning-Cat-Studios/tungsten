//! Cache management methods for BuildCache.
//!
//! Handles eviction, pruning, statistics, clearing, flushing,
//! and dependency graph operations.

use super::build::BuildCache;
use super::graph::{DependencyGraph, ModuleNode};
use super::types::{CacheStats, PruneStats};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::PathBuf;

impl BuildCache {
    /// Check cache size and evict if over limit.
    pub(super) fn maybe_evict(&mut self) -> io::Result<()> {
        let max_bytes = self.config.max_size_mb * 1024 * 1024;
        let current_size = self.compute_cache_size()?;

        if current_size <= max_bytes {
            return Ok(());
        }

        if self.verbose {
            eprintln!(
                "[cache] size {}MB exceeds limit {}MB, evicting...",
                current_size / (1024 * 1024),
                self.config.max_size_mb
            );
        }

        // Sort entries by last_accessed (oldest first).
        let mut entries: Vec<_> = self.manifest.entries.iter().collect();
        entries.sort_by_key(|(_, e)| e.last_accessed);

        // Remove oldest entries until under limit.
        let mut freed = 0u64;
        let mut to_remove = Vec::new();

        for (path, entry) in entries {
            if current_size - freed <= max_bytes {
                break;
            }

            let ast_path = self.cache_dir.join(&entry.ast_path);
            if let Ok(metadata) = fs::metadata(&ast_path) {
                freed += metadata.len();
                to_remove.push(path.clone());
                if self.verbose {
                    eprintln!("[cache] evicting: {}", path.display());
                }
                let _ = fs::remove_file(&ast_path);
            }
        }

        for path in to_remove {
            self.manifest.entries.remove(&path);
        }

        self.save_manifest()?;

        if self.verbose {
            eprintln!("[cache] evicted {}KB", freed / 1024);
        }

        Ok(())
    }

    /// Compute total size of cached files.
    fn compute_cache_size(&self) -> io::Result<u64> {
        let mut total = 0;
        for entry in self.manifest.entries.values() {
            let ast_path = self.cache_dir.join(&entry.ast_path);
            if let Ok(metadata) = fs::metadata(&ast_path) {
                total += metadata.len();
            }
        }
        Ok(total)
    }

    /// Get cache statistics.
    pub fn stats(&self) -> io::Result<CacheStats> {
        let size_bytes = self.compute_cache_size()?;
        let entry_count = self.manifest.entries.len();

        let oldest = self
            .manifest
            .entries
            .values()
            .map(|e| e.last_accessed)
            .min();
        let newest = self
            .manifest
            .entries
            .values()
            .map(|e| e.last_accessed)
            .max();

        Ok(CacheStats {
            size_bytes,
            entry_count,
            max_size_mb: self.config.max_size_mb,
            oldest_accessed: oldest,
            newest_accessed: newest,
        })
    }

    /// Flush any pending changes to disk.
    pub fn flush(&self) -> io::Result<()> {
        self.save_manifest()
    }

    /// Clear the entire cache.
    ///
    /// This removes all cached files and resets the manifest.
    pub fn clear(&mut self) -> io::Result<()> {
        // Remove all cached AST files
        let modules_dir = self.cache_dir.join("modules");
        if modules_dir.exists() {
            // Remove all files in modules directory
            for entry in fs::read_dir(&modules_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    fs::remove_file(&path)?;
                }
            }
        }

        // Reset manifest
        self.manifest.entries.clear();
        self.manifest.dependency_graph = DependencyGraph::new();
        self.save_manifest()?;

        Ok(())
    }

    /// Prune the cache to a target size.
    ///
    /// If `target_mb` is `None`, uses the configured `max_size_mb`.
    /// This removes the least recently used entries until the cache
    /// is at or below the target size.
    pub fn prune(&mut self, target_mb: Option<u64>) -> io::Result<PruneStats> {
        let target_bytes = target_mb.unwrap_or(self.config.max_size_mb) * 1024 * 1024;
        let current_size = self.compute_cache_size()?;

        if current_size <= target_bytes {
            return Ok(PruneStats {
                removed_count: 0,
                freed_bytes: 0,
                new_size_bytes: current_size,
            });
        }

        // Sort entries by last_accessed (oldest first).
        let mut entries: Vec<_> = self.manifest.entries.iter().collect();
        entries.sort_by_key(|(_, e)| e.last_accessed);

        // Remove oldest entries until under target.
        let mut freed = 0u64;
        let mut removed_count = 0usize;
        let mut to_remove = Vec::new();

        for (path, entry) in entries {
            if current_size - freed <= target_bytes {
                break;
            }

            // Remove AST file
            let ast_path = self.cache_dir.join(&entry.ast_path);
            if let Ok(metadata) = fs::metadata(&ast_path) {
                freed += metadata.len();
                let _ = fs::remove_file(&ast_path);
            }

            // Remove IR file if present
            if let Some(ref ir_rel_path) = entry.ir_path {
                let ir_path = self.cache_dir.join(ir_rel_path);
                if let Ok(metadata) = fs::metadata(&ir_path) {
                    freed += metadata.len();
                    let _ = fs::remove_file(&ir_path);
                }
            }

            to_remove.push(path.clone());
            removed_count += 1;
        }

        for path in to_remove {
            self.manifest.entries.remove(&path);
        }

        self.save_manifest()?;

        Ok(PruneStats {
            removed_count,
            freed_bytes: freed,
            new_size_bytes: current_size - freed,
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Dependency Graph Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Get a reference to the dependency graph.
    pub fn dependency_graph(&self) -> &DependencyGraph {
        &self.manifest.dependency_graph
    }

    /// Get a mutable reference to the dependency graph.
    pub fn dependency_graph_mut(&mut self) -> &mut DependencyGraph {
        &mut self.manifest.dependency_graph
    }

    /// Update the dependency graph with a new module tree.
    ///
    /// This rebuilds the entire graph from scratch based on the current module structure.
    pub fn update_dependency_graph(
        &mut self,
        root: PathBuf,
        modules: Vec<(PathBuf, [u8; 32], Vec<PathBuf>)>,
    ) {
        let mut graph = DependencyGraph::new();
        graph.set_root(root);

        // First pass: add all modules.
        for (path, content_hash, _) in &modules {
            graph.modules.insert(
                path.clone(),
                ModuleNode {
                    path: path.clone(),
                    content_hash: *content_hash,
                    dependencies: Vec::new(),
                    dependents: Vec::new(),
                },
            );
        }

        // Second pass: set dependencies and compute reverse edges.
        for (path, _, dependencies) in modules {
            if let Some(node) = graph.modules.get_mut(&path) {
                node.dependencies.clone_from(&dependencies);
            }

            // Update dependents for each dependency.
            for dep in dependencies {
                if let Some(dep_node) = graph.modules.get_mut(&dep) {
                    if !dep_node.dependents.contains(&path) {
                        dep_node.dependents.push(path.clone());
                    }
                }
            }
        }

        if self.verbose {
            eprintln!(
                "[cache] updated dependency graph: {} modules",
                graph.modules.len()
            );
        }

        self.manifest.dependency_graph = graph;
    }

    /// Compute which modules need to be invalidated based on content changes.
    ///
    /// Returns the set of module paths that need re-processing.
    pub fn compute_invalidation(
        &self,
        current_hashes: &HashMap<PathBuf, [u8; 32]>,
    ) -> HashSet<PathBuf> {
        let invalid = self
            .manifest
            .dependency_graph
            .compute_invalidation(current_hashes);

        if self.verbose && !invalid.is_empty() {
            eprintln!("[cache] invalidation cascade: {} modules", invalid.len());
            for path in &invalid {
                if let Some(filename) = path.file_name() {
                    eprintln!("[cache]   - {}", filename.to_string_lossy());
                }
            }
        }

        invalid
    }

    /// Compute the content hash for a file.
    pub fn compute_hash(content: &[u8]) -> [u8; 32] {
        Self::hash_content(content)
    }
}
