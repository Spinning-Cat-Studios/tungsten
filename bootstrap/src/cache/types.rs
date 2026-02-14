//! Data types for the build cache.

use super::graph::DependencyGraph;
use super::schema::{AST_SCHEMA_HASH, CACHE_FORMAT_VERSION, COMPILER_VERSION, DEFAULT_MAX_SIZE_MB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// A cached entry for a single source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// SHA-256 hash of the source file content.
    pub content_hash: [u8; 32],
    /// File modification time (for fast-path checking).
    pub mtime: Duration,
    /// Last time this entry was accessed (for LRU eviction).
    pub last_accessed: Duration,
    /// Path to the cached AST file (relative to cache dir).
    pub ast_path: PathBuf,
    /// Path to the cached IR file (relative to cache dir), if available.
    #[serde(default)]
    pub ir_path: Option<PathBuf>,
    /// Hash of type definitions used during elaboration (for IR invalidation).
    #[serde(default)]
    pub types_hash: Option<[u8; 32]>,
    /// IR schema version when IR was cached.
    #[serde(default)]
    pub ir_schema_version: Option<u32>,
}

/// The cache manifest tracking all cached files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManifest {
    /// Cache format version.
    pub version: u32,
    /// Compiler version that created this cache.
    pub compiler_version: String,
    /// AST schema hash - auto-invalidates cache when AST struct definitions change.
    /// Computed from AST_SCHEMA_SIGNATURE at compile time.
    #[serde(default)]
    pub ast_schema_hash: [u8; 32],
    /// Map from source file path (absolute) to cache entry.
    pub entries: HashMap<PathBuf, CacheEntry>,
    /// Module dependency graph (for invalidation).
    #[serde(default)]
    pub dependency_graph: DependencyGraph,
}

impl Default for CacheManifest {
    fn default() -> Self {
        Self {
            version: CACHE_FORMAT_VERSION,
            compiler_version: COMPILER_VERSION.to_string(),
            ast_schema_hash: AST_SCHEMA_HASH,
            entries: HashMap::new(),
            dependency_graph: DependencyGraph::new(),
        }
    }
}

/// Cache configuration loaded from `.tungsten/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum cache size in megabytes.
    #[serde(default = "default_max_size_mb")]
    pub max_size_mb: u64,
}

fn default_max_size_mb() -> u64 {
    DEFAULT_MAX_SIZE_MB
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size_mb: DEFAULT_MAX_SIZE_MB,
        }
    }
}

/// Cache statistics.
#[derive(Debug)]
pub struct CacheStats {
    /// Total size of cached files in bytes.
    pub size_bytes: u64,
    /// Number of cached entries.
    pub entry_count: usize,
    /// Maximum configured size in MB.
    pub max_size_mb: u64,
    /// Oldest entry's last access time.
    pub oldest_accessed: Option<Duration>,
    /// Newest entry's last access time.
    pub newest_accessed: Option<Duration>,
}

/// Statistics from a prune operation.
#[derive(Debug)]
pub struct PruneStats {
    /// Number of entries removed.
    pub removed_count: usize,
    /// Bytes freed.
    pub freed_bytes: u64,
    /// New cache size in bytes.
    pub new_size_bytes: u64,
}
