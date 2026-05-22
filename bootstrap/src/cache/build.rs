//! Build cache implementation.
//!
//! The `BuildCache` struct provides methods to cache and retrieve parsed ASTs,
//! with automatic invalidation based on content hashes.
//!
//! IR caching is in `ir_cache.rs`; eviction, pruning, and dependency graph
//! management are in `management.rs`.

use super::types::{CacheConfig, CacheEntry, CacheManifest};
use super::types::{AST_SCHEMA_HASH, CACHE_FORMAT_VERSION, COMPILER_VERSION};
use crate::ast::SourceFile;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The build cache instance.
pub struct BuildCache {
    /// Path to the .tungsten/cache directory.
    pub(super) cache_dir: PathBuf,
    /// Path to the manifest file.
    pub(super) manifest_path: PathBuf,
    /// The loaded manifest.
    pub(crate) manifest: CacheManifest,
    /// Cache configuration.
    pub(super) config: CacheConfig,
    /// Whether verbose logging is enabled.
    pub(super) verbose: bool,
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

        let config = Self::load_config(&config_path, verbose);
        let manifest = Self::load_or_init_manifest(&manifest_path, &cache_dir, verbose)?;

        Ok(Self {
            cache_dir,
            manifest_path,
            manifest,
            config,
            verbose,
        })
    }

    /// Load config from disk, falling back to defaults.
    fn load_config(config_path: &Path, verbose: bool) -> CacheConfig {
        if config_path.exists() {
            match fs::read_to_string(config_path) {
                Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                    if verbose {
                        eprintln!("[cache] warning: failed to parse config.toml: {e}");
                    }
                    CacheConfig::default()
                }),
                Err(_) => CacheConfig::default(),
            }
        } else {
            CacheConfig::default()
        }
    }

    /// Load an existing manifest if valid, or create a fresh one.
    fn load_or_init_manifest(
        manifest_path: &Path,
        cache_dir: &Path,
        verbose: bool,
    ) -> io::Result<CacheManifest> {
        if !manifest_path.exists() {
            if verbose {
                eprintln!("[cache] creating new cache");
            }
            return Ok(CacheManifest::default());
        }

        match Self::load_manifest(manifest_path) {
            Ok(m)
                if m.version == CACHE_FORMAT_VERSION
                    && m.compiler_version == COMPILER_VERSION
                    && m.ast_schema_hash == AST_SCHEMA_HASH =>
            {
                if verbose {
                    eprintln!("[cache] loaded manifest with {} entries", m.entries.len());
                }
                Ok(m)
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
                Self::reset_cache_dir(cache_dir)?;
                Ok(CacheManifest::default())
            }
            Err(e) => {
                if verbose {
                    eprintln!("[cache] warning: failed to load manifest: {e}");
                }
                Self::reset_cache_dir(cache_dir)?;
                Ok(CacheManifest::default())
            }
        }
    }

    /// Clear and re-create the cache directory structure.
    fn reset_cache_dir(cache_dir: &Path) -> io::Result<()> {
        let _ = fs::remove_dir_all(cache_dir);
        fs::create_dir_all(cache_dir)?;
        fs::create_dir_all(cache_dir.join("modules"))?;
        Ok(())
    }

    /// Load manifest from disk.
    fn load_manifest(path: &Path) -> io::Result<CacheManifest> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        bincode::deserialize_from(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Save manifest to disk.
    pub(super) fn save_manifest(&self) -> io::Result<()> {
        let file = File::create(&self.manifest_path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, &self.manifest).map_err(|e| io::Error::other(e))
    }

    /// Compute SHA-256 hash of content.
    pub(crate) fn hash_content(content: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hasher.finalize().into()
    }

    /// Convert SystemTime to Duration since UNIX_EPOCH.
    pub(super) fn system_time_to_duration(time: SystemTime) -> Duration {
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
        let file = match File::open(&ast_path) {
            Ok(f) => f,
            Err(e) => {
                if self.verbose {
                    eprintln!(
                        "[cache] warning: missing cache file for {}: {e}",
                        source_path.display()
                    );
                }
                return None;
            }
        };

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
        bincode::serialize_into(writer, ast).map_err(|e| io::Error::other(e))?;

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
}
