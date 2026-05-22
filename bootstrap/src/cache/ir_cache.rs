//! IR (Elaboration) cache methods for BuildCache.
//!
//! Handles caching and retrieval of elaborated IR, with invalidation
//! based on type definition hashes and IR schema versioning.

use super::build::BuildCache;
use super::types::IR_SCHEMA_VERSION;
use crate::elaborate::CoreDef;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::time::SystemTime;

impl BuildCache {
    /// Get cached elaborated IR for a source file.
    ///
    /// Returns None if:
    /// - No IR cache exists for this file
    /// - The types_hash doesn't match (type definitions changed)
    /// - The IR schema version doesn't match
    /// - The cache is corrupt
    pub fn get_ir(
        &mut self,
        source_path: &std::path::Path,
        types_hash: &[u8; 32],
    ) -> Option<Vec<CoreDef>> {
        let abs_path = match source_path.canonicalize() {
            Ok(p) => p,
            Err(_) => return None,
        };

        let entry = self.manifest.entries.get(&abs_path)?;

        // Check if IR exists
        let ir_path = entry.ir_path.clone()?;
        let cached_types_hash = entry.types_hash?;
        let cached_schema = entry.ir_schema_version?;

        // Check types_hash matches
        if &cached_types_hash != types_hash {
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

        // Load the IR from disk
        let full_ir_path = self.cache_dir.join(ir_path);
        self.load_ir_from_disk(&full_ir_path, source_path, &abs_path, &cached_types_hash)
    }

    /// Load and deserialize IR from a cache file on disk.
    fn load_ir_from_disk(
        &mut self,
        full_ir_path: &PathBuf,
        source_path: &std::path::Path,
        abs_path: &PathBuf,
        cached_types_hash: &[u8; 32],
    ) -> Option<Vec<CoreDef>> {
        let file = match File::open(full_ir_path) {
            Ok(f) => f,
            Err(e) => {
                if self.verbose {
                    eprintln!(
                        "[cache] warning: missing IR cache file for {}: {e}",
                        source_path.display()
                    );
                }
                return None;
            }
        };
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
                if let Some(e) = self.manifest.entries.get_mut(abs_path) {
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

    /// Cache elaborated IR for a source file.
    ///
    /// The types_hash should be computed from all type definitions visible
    /// during elaboration of this module.
    pub fn put_ir(
        &mut self,
        source_path: &std::path::Path,
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
        bincode::serialize_into(writer, defs).map_err(|e| io::Error::other(e))?;

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
}
