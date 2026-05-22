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
pub mod elab_cache;
mod graph;
mod ir_cache;
mod management;
mod types;

pub use build::BuildCache;
pub use elab_cache::{
    compute_full_output_cache_key, compute_module_cache_key, hash_exports, hash_file_content,
    CachedModuleFullOutput, CachedModuleSignature, ElabCacheStats, FULL_OUTPUT_SCHEMA_VERSION,
};
pub use graph::DependencyGraph;
pub use types::{
    compute_schema_hash, CacheConfig, CacheEntry, CacheManifest, CacheStats, PruneStats,
    AST_SCHEMA_HASH, AST_SCHEMA_SIGNATURE, CACHE_FORMAT_VERSION, COMPILER_VERSION,
    DEFAULT_MAX_SIZE_MB, IR_SCHEMA_VERSION,
};

#[cfg(test)]
mod tests;
