//! Build cache implementation.
//!
//! The `BuildCache` struct provides methods to cache and retrieve parsed ASTs
//! and elaborated IR, with automatic invalidation based on content hashes,
//! type changes, and dependency graphs.

use super::graph::{DependencyGraph, ModuleNode};
use super::schema::{AST_SCHEMA_HASH, CACHE_FORMAT_VERSION, COMPILER_VERSION, IR_SCHEMA_VERSION};
use super::types::{CacheConfig, CacheEntry, CacheManifest, CacheStats, PruneStats};
use crate::ast::SourceFile;
use crate::elaborate::CoreDef;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The build cache instance.
pub struct BuildCache {
    /// Path to the .tungsten/cache directory.
    cache_dir: PathBuf,
    /// Path to the manifest file.
    manifest_path: PathBuf,
    /// The loaded manifest.
    pub(crate) manifest: CacheManifest,
    /// Cache configuration.
    config: CacheConfig,
    /// Whether verbose logging is enabled.
    verbose: bool,
}

impl BuildCache {
    /// Create or load a build cache for the given project root.
    pub fn new(project_root: &Path, verbose: bool) -> io::Result<Self> {
        let tungsten_dir = project_root.join(".tungsten");
        let cache_dir = tungsten_dir.join("cache");
        let manifest_path = cache_dir.join("manifest.bin");
        let config_path = tungsten_dir.join("config.toml");

        // Ensure directories exist.
        fs::create_dir_all(&cache_dir)?;
        fs::create_dir_all(cache_dir.join("modules"))?;

        // Load config if it exists.
        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            toml::from_str(&content).unwrap_or_else(|e| {
                if verbose {
                    eprintln!("[cache] warning: failed to parse config.toml: {e}");
                }
                CacheConfig::default()
            })
        } else {
            CacheConfig::default()
        };

        // Load manifest if it exists and is compatible.
        let manifest = if manifest_path.exists() {
            match Self::load_manifest(&manifest_path) {
                Ok(m)
                    if m.version == CACHE_FORMAT_VERSION
                        && m.compiler_version == COMPILER_VERSION
                        && m.ast_schema_hash == AST_SCHEMA_HASH =>
                {
                    if verbose {
                        eprintln!("[cache] loaded manifest with {} entries", m.entries.len());
                    }
                    m
                }
                Ok(m) => {
                    if verbose {
                        eprintln!(
                            "[cache] invalidating cache: schema mismatch (cache: v{} compiler {} ast_hash {:02x}{:02x}{:02x}{:02x}..., current: v{} compiler {} ast_hash {:02x}{:02x}{:02x}{:02x}...)",
                            m.version, m.compiler_version,
                            m.ast_schema_hash[0], m.ast_schema_hash[1], m.ast_schema_hash[2], m.ast_schema_hash[3],
                            CACHE_FORMAT_VERSION, COMPILER_VERSION,
                            AST_SCHEMA_HASH[0], AST_SCHEMA_HASH[1], AST_SCHEMA_HASH[2], AST_SCHEMA_HASH[3]
                        );
                    }
                    // Clear old cache on version/schema mismatch.
                    let _ = fs::remove_dir_all(&cache_dir);
                    fs::create_dir_all(&cache_dir)?;
                    fs::create_dir_all(cache_dir.join("modules"))?;
                    CacheManifest::default()
                }
                Err(e) => {
                    if verbose {
                        eprintln!("[cache] warning: failed to load manifest: {e}");
                    }
                    // Failed to deserialize - likely corrupt or very old format.
                    // Clear cache and start fresh.
                    let _ = fs::remove_dir_all(&cache_dir);
                    fs::create_dir_all(&cache_dir)?;
                    fs::create_dir_all(cache_dir.join("modules"))?;
                    CacheManifest::default()
                }
            }
        } else {
            if verbose {
                eprintln!("[cache] creating new cache");
            }
            CacheManifest::default()
        };

        Ok(Self {
            cache_dir,
            manifest_path,
            manifest,
            config,
            verbose,
        })
    }

    /// Load manifest from disk.
    fn load_manifest(path: &Path) -> io::Result<CacheManifest> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        bincode::deserialize_from(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Save manifest to disk.
    fn save_manifest(&self) -> io::Result<()> {
        let file = File::create(&self.manifest_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, &self.manifest)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    /// Compute SHA-256 hash of content.
    pub(crate) fn hash_content(content: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hasher.finalize().into()
    }

    /// Convert SystemTime to Duration since UNIX_EPOCH.
    fn system_time_to_duration(time: SystemTime) -> Duration {
        time.duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // AST Cache Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Try to get cached AST for a source file.
    ///
    /// Returns `Some(ast)` on cache hit, `None` on miss.
    pub fn get(&mut self, source_path: &Path, source_content: &str) -> Option<SourceFile> {
        let abs_path = source_path.canonicalize().ok()?;

        // Clone entry data we need to avoid borrow issues.
        let (content_hash, mtime, ast_path) = {
            let entry = self.manifest.entries.get(&abs_path)?;
            (entry.content_hash, entry.mtime, entry.ast_path.clone())
        };

        // Always verify content hash matches to catch changes within same mtime second.
        // This is important because filesystem mtime often has 1-second resolution,
        // so rapid writes (common in tests) won't update mtime.
        let computed_hash = Self::hash_content(source_content.as_bytes());
        if computed_hash != content_hash {
            // Hash changed → cache miss.
            if self.verbose {
                eprintln!("[cache] miss: {} (content changed)", source_path.display());
            }
            return None;
        }

        // Fast-path: check mtime to skip loading AST if nothing changed.
        if let Ok(metadata) = fs::metadata(source_path) {
            if let Ok(file_mtime) = metadata.modified() {
                let mtime_dur = Self::system_time_to_duration(file_mtime);
                if mtime_dur == mtime {
                    // mtime and hash match, load cached AST directly.
                    return self.load_cached_ast_from_path(
                        source_path,
                        &abs_path,
                        &content_hash,
                        &ast_path,
                    );
                }
            }
        }

        // mtime differs but hash matches (file was touched), still a hit.
        self.load_cached_ast_from_path(source_path, &abs_path, &content_hash, &ast_path)
    }

    /// Load cached AST from disk.
    fn load_cached_ast_from_path(
        &mut self,
        source_path: &Path,
        abs_path: &Path,
        content_hash: &[u8; 32],
        ast_rel_path: &Path,
    ) -> Option<SourceFile> {
        let ast_path = self.cache_dir.join(ast_rel_path);
        match File::open(&ast_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                match bincode::deserialize_from::<_, SourceFile>(reader) {
                    Ok(ast) => {
                        if self.verbose {
                            let hash_hex: String = content_hash
                                .iter()
                                .take(8)
                                .map(|b| format!("{b:02x}"))
                                .collect();
                            eprintln!(
                                "[cache] hit: {} (hash: {}...)",
                                source_path.display(),
                                hash_hex
                            );
                        }
                        // Update last_accessed.
                        if let Some(e) = self.manifest.entries.get_mut(abs_path) {
                            e.last_accessed = Self::system_time_to_duration(SystemTime::now());
                        }
                        Some(ast)
                    }
                    Err(e) => {
                        if self.verbose {
                            eprintln!(
                                "[cache] warning: corrupt cache for {}: {e}",
                                source_path.display()
                            );
                        }
                        None
                    }
                }
            }
            Err(e) => {
                if self.verbose {
                    eprintln!(
                        "[cache] warning: missing cache file for {}: {e}",
                        source_path.display()
                    );
                }
                None
            }
        }
    }

    /// Cache a parsed AST.
    pub fn put(
        &mut self,
        source_path: &Path,
        source_content: &str,
        ast: &SourceFile,
    ) -> io::Result<()> {
        let abs_path = source_path.canonicalize()?;
        let content_hash = Self::hash_content(source_content.as_bytes());

        // Generate unique filename from hash.
        let hash_hex: String = content_hash.iter().map(|b| format!("{b:02x}")).collect();
        let ast_filename = format!("{hash_hex}.ast.bin");
        let ast_path = PathBuf::from("modules").join(&ast_filename);
        let full_ast_path = self.cache_dir.join(&ast_path);

        // Serialize AST.
        let file = File::create(&full_ast_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, ast)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Get mtime.
        let mtime = fs::metadata(source_path)?
            .modified()
            .map(Self::system_time_to_duration)
            .unwrap_or(Duration::ZERO);

        // Create entry.
        let entry = CacheEntry {
            content_hash,
            mtime,
            last_accessed: Self::system_time_to_duration(SystemTime::now()),
            ast_path,
            ir_path: None,
            types_hash: None,
            ir_schema_version: None,
        };

        if self.verbose {
            eprintln!(
                "[cache] wrote: {} (hash: {}...)",
                source_path.display(),
                &hash_hex[..16]
            );
        }

        self.manifest.entries.insert(abs_path, entry);
        self.save_manifest()?;

        // Check if we need LRU eviction.
        self.maybe_evict()?;

        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // IR (Elaboration) Cache Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Get cached elaborated IR for a source file.
    ///
    /// Returns None if:
    /// - No IR cache exists for this file
    /// - The types_hash doesn't match (type definitions changed)
    /// - The IR schema version doesn't match
    /// - The cache is corrupt
    pub fn get_ir(&mut self, source_path: &Path, types_hash: &[u8; 32]) -> Option<Vec<CoreDef>> {
        let abs_path = match source_path.canonicalize() {
            Ok(p) => p,
            Err(_) => return None,
        };

        let entry = self.manifest.entries.get(&abs_path)?;

        // Check if IR exists
        let ir_path = entry.ir_path.as_ref()?;
        let cached_types_hash = entry.types_hash.as_ref()?;
        let cached_schema = entry.ir_schema_version?;

        // Check types_hash matches
        if cached_types_hash != types_hash {
            if self.verbose {
                eprintln!("[cache] ir miss: {} (types changed)", source_path.display());
            }
            return None;
        }

        // Check IR schema version
        if cached_schema != IR_SCHEMA_VERSION {
            if self.verbose {
                eprintln!(
                    "[cache] ir miss: {} (schema version {} != {})",
                    source_path.display(),
                    cached_schema,
                    IR_SCHEMA_VERSION
                );
            }
            return None;
        }

        // Load the IR
        let full_ir_path = self.cache_dir.join(ir_path);
        match File::open(&full_ir_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                match bincode::deserialize_from::<_, Vec<CoreDef>>(reader) {
                    Ok(defs) => {
                        if self.verbose {
                            let hash_hex: String = cached_types_hash
                                .iter()
                                .take(8)
                                .map(|b| format!("{b:02x}"))
                                .collect();
                            eprintln!(
                                "[cache] ir hit: {} (types_hash: {}...)",
                                source_path.display(),
                                hash_hex
                            );
                        }
                        // Update last_accessed
                        if let Some(e) = self.manifest.entries.get_mut(&abs_path) {
                            e.last_accessed = Self::system_time_to_duration(SystemTime::now());
                        }
                        Some(defs)
                    }
                    Err(e) => {
                        if self.verbose {
                            eprintln!(
                                "[cache] warning: corrupt IR cache for {}: {e}",
                                source_path.display()
                            );
                        }
                        None
                    }
                }
            }
            Err(e) => {
                if self.verbose {
                    eprintln!(
                        "[cache] warning: missing IR cache file for {}: {e}",
                        source_path.display()
                    );
                }
                None
            }
        }
    }

    /// Cache elaborated IR for a source file.
    ///
    /// The types_hash should be computed from all type definitions visible
    /// during elaboration of this module.
    pub fn put_ir(
        &mut self,
        source_path: &Path,
        types_hash: [u8; 32],
        defs: &[CoreDef],
    ) -> io::Result<()> {
        let abs_path = source_path.canonicalize()?;

        // Check if we have a cache entry for this file
        if !self.manifest.entries.contains_key(&abs_path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no AST cache entry for this file (put AST first)",
            ));
        }

        // Generate IR filename from content hash + types hash
        let entry = self.manifest.entries.get(&abs_path).unwrap();
        let content_hash = entry.content_hash;
        let combined_hash =
            Self::hash_content(&[content_hash.as_slice(), types_hash.as_slice()].concat());
        let hash_hex: String = combined_hash.iter().map(|b| format!("{b:02x}")).collect();
        let ir_filename = format!("{hash_hex}.ir.bin");
        let ir_rel_path = PathBuf::from("modules").join(&ir_filename);
        let full_ir_path = self.cache_dir.join(&ir_rel_path);

        // Serialize IR
        let file = File::create(&full_ir_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, defs)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Update entry
        if let Some(entry) = self.manifest.entries.get_mut(&abs_path) {
            entry.ir_path = Some(ir_rel_path);
            entry.types_hash = Some(types_hash);
            entry.ir_schema_version = Some(IR_SCHEMA_VERSION);
            entry.last_accessed = Self::system_time_to_duration(SystemTime::now());
        }

        if self.verbose {
            let types_hash_hex: String = types_hash
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect();
            eprintln!(
                "[cache] ir wrote: {} (types_hash: {}...)",
                source_path.display(),
                types_hash_hex
            );
        }

        self.save_manifest()?;

        // Check if we need LRU eviction
        self.maybe_evict()?;

        Ok(())
    }

    /// Compute a hash of all type definitions for IR cache invalidation.
    ///
    /// This should be called after the collection pass with all type definitions.
    pub fn compute_types_hash(types: &[(String, crate::elaborate::TypeDef)]) -> [u8; 32] {
        let mut hasher = Sha256::new();

        // Sort by name for deterministic ordering
        let mut sorted: Vec<_> = types.iter().collect();
        sorted.sort_by_key(|(name, _)| name.as_str());

        for (name, typedef) in sorted {
            hasher.update(name.as_bytes());
            // Hash the serialized typedef
            if let Ok(bytes) = bincode::serialize(typedef) {
                hasher.update(&bytes);
            }
        }

        hasher.finalize().into()
    }

    /// Get the current IR schema version.
    pub fn ir_schema_version() -> u32 {
        IR_SCHEMA_VERSION
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Cache Management Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Check cache size and evict if over limit.
    fn maybe_evict(&mut self) -> io::Result<()> {
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
                node.dependencies = dependencies.clone();
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
