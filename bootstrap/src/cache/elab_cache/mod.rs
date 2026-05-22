//! Per-module elaboration cache (ADR 10.5.26l, 10.5.26n, 12.5.26a).
//!
//! Caches Phase B signature data (types, exports, metadata) for individual
//! modules so unchanged modules can skip re-elaboration. Phases A and A.5
//! always run (they are cheap).
//!
//! Full-output caching (ADR 12.5.26a): When `TUNGSTEN_ELAB_CACHE_FULL=1` is
//! set, CoreDef term bodies are also cached, allowing warm builds to skip body
//! elaboration entirely. This is opt-in and best-effort — failures fall back to
//! fresh elaboration.

#[cfg(feature = "compress")]
pub mod compress;

pub mod writer;

mod full_output;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use crate::elaborate::{ElabError, ElabOutput, ModuleExports, TypeProvenance};
use tungsten_core::Type;

use super::build::BuildCache;

/// Schema version for full-output cache entries (ADR 12.5.26a).
/// Bump when the serialization format of `CachedModuleFullOutput` changes.
pub const FULL_OUTPUT_SCHEMA_VERSION: u32 = 1;

/// Full-output cache entry for a single module (ADR 12.5.26a).
///
/// Contains the complete elaborated definitions (CoreDef bodies + type metadata)
/// so downstream consumers can skip body elaboration entirely on cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedModuleFullOutput {
    /// All elaborated definitions from this module.
    pub defs: Vec<crate::elaborate::CoreDef>,
    /// Type metadata needed to reconstruct the full ElabOutput.
    pub record_types: std::collections::HashMap<String, Vec<(String, Type)>>,
    pub adt_types:
        std::collections::HashMap<String, (Vec<String>, Vec<crate::elaborate::Constructor>)>,
    pub type_aliases: std::collections::HashMap<String, (Vec<String>, Type)>,
    pub type_provenance: TypeProvenance,
    pub encoded_types: std::collections::HashMap<String, Type>,
    pub mutual_recursion_groups: std::collections::HashMap<String, Vec<String>>,
    /// Delta exports (same as signature cache).
    pub delta_exports: ModuleExports,
    /// Interface-level warnings to replay on cache hit.
    pub warnings: Vec<ElabError>,
}

/// Signature-only cache entry for a single module (ADR 10.5.26n §2.1).
///
/// Contains only what downstream modules need to seed their environments:
/// exports, type metadata, and interface-level warnings. Does NOT contain
/// `defs: Vec<CoreDef>` — those are regenerated during fresh elaboration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedModuleSignature {
    /// Delta exports: only the types/values/constructors added by THIS module
    /// (not inherited from prior modules). On cache hit, these are merged with
    /// the current accumulated exports to reconstruct the full export set.
    pub delta_exports: ModuleExports,
    /// Interface-level warnings to replay on cache hit.
    pub warnings: Vec<ElabError>,
    /// Number of definitions elaborated (for reporting on cache hit).
    pub def_count: usize,
}

impl CachedModuleSignature {
    /// Build a signature storing only the delta exports (ADR 10.5.26n).
    ///
    /// Computes the difference between `full_exports` (all env entries after
    /// elaboration) and `prior_exports` (entries injected from prior modules).
    /// Only new entries added by this module are cached, keeping each entry
    /// small (~200B–2KB) instead of storing the full accumulated env (~1.8MB).
    pub fn from_output(
        output: &ElabOutput,
        full_exports: &ModuleExports,
        prior_exports: &ModuleExports,
    ) -> Self {
        Self {
            delta_exports: compute_delta_exports(full_exports, prior_exports),
            warnings: output.warnings.clone(),
            def_count: output.defs.len(),
        }
    }

    /// Reconstruct a minimal ElabOutput for accumulation (empty defs + type maps).
    pub fn into_elab_output(self) -> ElabOutput {
        ElabOutput {
            defs: Vec::new(),
            warnings: self.warnings,
            record_types: HashMap::new(),
            adt_types: HashMap::new(),
            type_aliases: HashMap::new(),
            type_provenance: TypeProvenance::default(),
            encoded_types: HashMap::new(),
            mutual_recursion_groups: HashMap::new(),
            type_visibilities: HashMap::new(),
            record_field_visibilities: HashMap::new(),
        }
    }
}

/// Compute delta exports: entries in `full` that are NOT in `prior`.
fn compute_delta_exports(full: &ModuleExports, prior: &ModuleExports) -> ModuleExports {
    use std::collections::HashSet;
    let prior_types: HashSet<&str> = prior.types.iter().map(|(n, _)| n.as_str()).collect();
    let prior_values: HashSet<&str> = prior.values.iter().map(|(n, _)| n.as_str()).collect();
    let prior_ctors: HashSet<&str> = prior.constructors.iter().map(|(n, _)| n.as_str()).collect();

    ModuleExports {
        types: full
            .types
            .iter()
            .filter(|(n, _)| !prior_types.contains(n.as_str()))
            .cloned()
            .collect(),
        values: full
            .values
            .iter()
            .filter(|(n, _)| !prior_values.contains(n.as_str()))
            .cloned()
            .collect(),
        constructors: full
            .constructors
            .iter()
            .filter(|(n, _)| !prior_ctors.contains(n.as_str()))
            .cloned()
            .collect(),
    }
}

/// Compute a cache key for a single module's Phase B elaboration.
///
/// The key incorporates:
/// - Compiler version (via `COMPILER_VERSION`)
/// - File content hash (SHA-256)
/// - Phase A.5 exports hash (captures transitive dependency state)
///
/// Using the Phase A.5 exports hash as a proxy for transitive imports
/// is conservative: any change in any module's types/values/constructors
/// will change the exports hash and invalidate all downstream caches.
/// This is simpler than per-file transitive import hashing and correct
/// (over-invalidation is acceptable per §2.6).
pub fn compute_module_cache_key(
    compiler_version: &str,
    file_content_hash: &[u8; 32],
    exports_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"tungsten-elab-cache-v1:");
    hasher.update(compiler_version.as_bytes());
    hasher.update(b":");
    hasher.update(file_content_hash);
    hasher.update(b":");
    hasher.update(exports_hash);
    hasher.finalize().into()
}

/// Compute a content hash for a source file.
pub fn hash_file_content(content: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hasher.finalize().into()
}

/// Compute a hash of the current Phase A.5 exports state.
///
/// This captures the structural fingerprint of the type/value/constructor
/// environment that a module elaborates against. Uses names, kinds, arities,
/// and counts rather than full serialization to avoid non-determinism from
/// HashMap iteration order in the elaboration environment.
///
/// Any change that adds, removes, or renames a type/value/constructor, or
/// changes a type's kind/arity, will produce a different hash. This is an
/// over-invalidation strategy (see §2.6): correctness over precision.
pub fn hash_exports(exports: &ModuleExports) -> [u8; 32] {
    let mut hasher = Sha256::new();

    // Hash types: sorted by name, include kind tag + param count + ctor/field counts
    let mut type_entries: Vec<_> = exports
        .types
        .iter()
        .map(|(n, def)| {
            let kind_info = match &def.kind {
                crate::elaborate::TypeDefKind::Alias(_) => "alias:0".to_string(),
                crate::elaborate::TypeDefKind::ADT(ctors) => format!("adt:{}", ctors.len()),
                crate::elaborate::TypeDefKind::Record(fields) => format!("record:{}", fields.len()),
                crate::elaborate::TypeDefKind::Stub => "stub:0".to_string(),
            };
            (n.as_str(), kind_info, def.params.len())
        })
        .collect();
    type_entries.sort();
    hasher.update(b"types:");
    for (name, kind_info, param_count) in &type_entries {
        hasher.update(format!("{name}:{kind_info}:{param_count},").as_bytes());
    }

    // Hash values: sorted by name only (type display may be non-deterministic
    // due to HashMap-backed elaboration environments)
    let mut value_names: Vec<&str> = exports.values.iter().map(|(n, _)| n.as_str()).collect();
    value_names.sort();
    hasher.update(b"values:");
    for name in &value_names {
        hasher.update(format!("{name},").as_bytes());
    }

    // Hash constructors: sorted by name, include parent + arity
    let mut ctor_entries: Vec<_> = exports
        .constructors
        .iter()
        .map(|(n, info)| (n.as_str(), info.type_name.as_str(), info.arity))
        .collect();
    ctor_entries.sort();
    hasher.update(b"ctors:");
    for (name, parent, arity) in &ctor_entries {
        hasher.update(format!("{name}:{parent}:{arity},").as_bytes());
    }

    hasher.finalize().into()
}

impl CachedModuleFullOutput {
    /// Build a full-output entry from elaboration results (ADR 12.5.26a).
    pub fn from_output(
        output: &ElabOutput,
        full_exports: &ModuleExports,
        prior_exports: &ModuleExports,
    ) -> Self {
        Self {
            defs: output.defs.clone(),
            record_types: output.record_types.clone(),
            adt_types: output.adt_types.clone(),
            type_aliases: output.type_aliases.clone(),
            type_provenance: output.type_provenance.clone(),
            encoded_types: output.encoded_types.clone(),
            mutual_recursion_groups: output.mutual_recursion_groups.clone(),
            delta_exports: compute_delta_exports(full_exports, prior_exports),
            warnings: output.warnings.clone(),
        }
    }

    /// Reconstruct a full ElabOutput from the cached entry.
    pub fn into_elab_output(self) -> ElabOutput {
        ElabOutput {
            defs: self.defs,
            warnings: self.warnings,
            record_types: self.record_types,
            adt_types: self.adt_types,
            type_aliases: self.type_aliases,
            type_provenance: self.type_provenance,
            encoded_types: self.encoded_types,
            mutual_recursion_groups: self.mutual_recursion_groups,
            type_visibilities: HashMap::new(),
            record_field_visibilities: HashMap::new(),
        }
    }
}

/// Compute a full-output cache key extending the signature key (ADR 12.5.26a).
///
/// Extends the signature-only key with a full-output schema version to ensure
/// format changes invalidate full-output entries independently of signature entries.
pub fn compute_full_output_cache_key(
    signature_cache_key: &[u8; 32],
    full_output_schema_version: u32,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"tungsten-elab-full-v1:");
    hasher.update(signature_cache_key);
    hasher.update(b":");
    hasher.update(full_output_schema_version.to_le_bytes());
    hasher.finalize().into()
}

impl BuildCache {
    /// Look up a cached module signature (ADR 10.5.26n).
    ///
    /// Returns `None` on miss (no entry, hash mismatch, corrupt data).
    pub fn get_module_elab(&self, cache_key: &[u8; 32]) -> Option<CachedModuleSignature> {
        let key_hex = hex_string(cache_key);
        let path = self.cache_dir.join("elab").join(format!("{key_hex}.bin"));

        let file = File::open(&path).ok()?;
        let reader = BufReader::new(file);
        match bincode::deserialize_from::<_, CachedModuleSignature>(reader) {
            Ok(cached) => {
                if self.verbose {
                    eprintln!("[elab-cache] hit: {}", &key_hex[..16]);
                }
                Some(cached)
            }
            Err(e) => {
                if self.verbose {
                    eprintln!("[elab-cache] corrupt entry {}: {e}", &key_hex[..16]);
                }
                // Remove corrupt file
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }

    /// Store a module's signature in the cache (ADR 10.5.26n).
    pub fn put_module_elab(
        &self,
        cache_key: &[u8; 32],
        signature: &CachedModuleSignature,
    ) -> io::Result<()> {
        let elab_dir = self.cache_dir.join("elab");
        std::fs::create_dir_all(&elab_dir)?;

        let key_hex = hex_string(cache_key);
        let path = elab_dir.join(format!("{key_hex}.bin"));

        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, signature).map_err(|e| io::Error::other(e))?;

        if self.verbose {
            eprintln!("[elab-cache] wrote: {}", &key_hex[..16]);
        }

        Ok(())
    }

    /// Remove all cached elaboration entries.
    pub fn clean_elab_cache(&self) -> io::Result<()> {
        let elab_dir = self.cache_dir.join("elab");
        if elab_dir.exists() {
            std::fs::remove_dir_all(&elab_dir)?;
        }
        Ok(())
    }

    /// Get statistics about the elaboration cache.
    pub fn elab_cache_stats(&self) -> io::Result<ElabCacheStats> {
        let elab_dir = self.cache_dir.join("elab");
        let mut sig_count = 0usize;
        let mut sig_bytes = 0u64;
        let mut full_count = 0usize;
        let mut full_bytes = 0u64;

        if elab_dir.exists() {
            for entry in std::fs::read_dir(&elab_dir)? {
                let entry = entry?;
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with(".full.bin") || name.ends_with(".full.bin.zst") {
                    full_count += 1;
                    if let Ok(meta) = std::fs::metadata(&path) {
                        full_bytes += meta.len();
                    }
                } else if path.extension().map_or(false, |e| e == "bin") {
                    sig_count += 1;
                    if let Ok(meta) = std::fs::metadata(&path) {
                        sig_bytes += meta.len();
                    }
                }
            }
        }

        Ok(ElabCacheStats {
            entry_count: sig_count,
            size_bytes: sig_bytes,
            full_output_count: full_count,
            full_output_bytes: full_bytes,
        })
    }
}

/// Statistics for the elaboration cache.
#[derive(Debug)]
pub struct ElabCacheStats {
    /// Number of cached signature entries.
    pub entry_count: usize,
    /// Total size of signature cache files in bytes.
    pub size_bytes: u64,
    /// Number of cached full-output entries (ADR 12.5.26a).
    pub full_output_count: usize,
    /// Total size of full-output cache files in bytes.
    pub full_output_bytes: u64,
}

/// Format a hash as a hex string.
pub(crate) fn hex_string(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod full_output_tests;
