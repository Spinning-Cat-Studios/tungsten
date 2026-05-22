//! Full-output cache operations (ADR 12.5.26a, 10.5.26o).
//!
//! Handles reading, writing, and serializing full-output cache entries
//! (complete CoreDef bodies + type metadata) with optional zstd compression.

use std::io;
use std::path::PathBuf;

use super::{hex_string, CachedModuleFullOutput};

#[cfg(feature = "compress")]
use super::compress;

use crate::cache::build::BuildCache;

impl BuildCache {
    /// Look up a cached full-output entry (ADR 12.5.26a, 10.5.26o).
    ///
    /// Returns `Some(cached)` on hit, `None` on miss or corrupt entry.
    /// On corrupt entry, removes the bad file and returns `None`.
    /// Checks for compressed `.full.bin.zst` first, falls back to `.full.bin`.
    pub fn get_module_full_output(&self, full_key: &[u8; 32]) -> Option<CachedModuleFullOutput> {
        use bincode::Options;

        let key_hex = hex_string(full_key);
        let elab_dir = self.cache_dir.join("elab");

        // Try compressed entry first (ADR 10.5.26o)
        let zst_path = elab_dir.join(format!("{key_hex}.full.bin.zst"));
        let (bytes, path) = if zst_path.exists() {
            match Self::read_compressed_entry(&zst_path) {
                Ok(decompressed) => (decompressed, zst_path),
                Err(_) => {
                    if self.verbose {
                        eprintln!("[elab-cache-full] decompress failed: {}", &key_hex[..16]);
                    }
                    let _ = std::fs::remove_file(&zst_path);
                    return None;
                }
            }
        } else {
            // Fall back to uncompressed entry
            let bin_path = elab_dir.join(format!("{key_hex}.full.bin"));
            match std::fs::read(&bin_path) {
                Ok(data) => (data, bin_path),
                Err(_) => return None,
            }
        };

        // NOTE: `bincode::serialize` / `serialize_into` use fixint encoding by default,
        // but `bincode::options()` defaults to varint. We must explicitly select fixint
        // here to match the format produced by `bincode::serialize` in `put_module_full_output`
        // and `serialize_full_output_entry`. Mixing these causes silent deserialization failures.
        let opts = bincode::options()
            .with_fixint_encoding()
            .allow_trailing_bytes()
            .with_limit(512 * 1024 * 1024); // 512 MB
        match opts.deserialize::<CachedModuleFullOutput>(&bytes) {
            Ok(cached) => {
                if self.verbose {
                    eprintln!(
                        "[elab-cache-full] hit: {} ({} defs)",
                        &key_hex[..16],
                        cached.defs.len()
                    );
                }
                Some(cached)
            }
            Err(e) => {
                if self.verbose {
                    eprintln!("[elab-cache-full] corrupt entry {}: {e}", &key_hex[..16]);
                }
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }

    /// Read and decompress a zstd-compressed cache entry.
    #[cfg(feature = "compress")]
    fn read_compressed_entry(path: &std::path::Path) -> io::Result<Vec<u8>> {
        let compressed = std::fs::read(path)?;
        compress::decompress(&compressed)
    }

    /// Fallback when compress feature is not enabled — treat .zst as unreadable.
    #[cfg(not(feature = "compress"))]
    fn read_compressed_entry(_path: &std::path::Path) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "compressed cache entries require the 'compress' feature",
        ))
    }

    /// Serialize a full-output entry to bytes, optionally compressed (ADR 10.5.26o).
    ///
    /// When the `compress` feature is enabled, returns zstd-compressed bytes
    /// and a `.full.bin.zst` extension. Otherwise returns raw bincode bytes
    /// with a `.full.bin` extension.
    pub fn serialize_full_output_entry(
        &self,
        full_key: &[u8; 32],
        entry: &CachedModuleFullOutput,
    ) -> io::Result<(PathBuf, Vec<u8>)> {
        let elab_dir = self.cache_dir.join("elab");
        let key_hex = hex_string(full_key);
        let raw_bytes = bincode::serialize(entry).map_err(|e| io::Error::other(e))?;

        #[cfg(feature = "compress")]
        {
            let compressed = compress::compress(&raw_bytes)?;
            let path = elab_dir.join(format!("{key_hex}.full.bin.zst"));
            if self.verbose {
                let ratio = if compressed.is_empty() {
                    0.0
                } else {
                    raw_bytes.len() as f64 / compressed.len() as f64
                };
                eprintln!(
                    "[elab-cache-full] serialized: {} ({} defs, {:.1}× compression)",
                    &key_hex[..16],
                    entry.defs.len(),
                    ratio
                );
            }
            Ok((path, compressed))
        }

        #[cfg(not(feature = "compress"))]
        {
            let path = elab_dir.join(format!("{key_hex}.full.bin"));
            if self.verbose {
                eprintln!(
                    "[elab-cache-full] serialized: {} ({} defs)",
                    &key_hex[..16],
                    entry.defs.len()
                );
            }
            Ok((path, raw_bytes))
        }
    }

    /// Store a module's full output in the cache (ADR 12.5.26a).
    pub fn put_module_full_output(
        &self,
        full_key: &[u8; 32],
        entry: &CachedModuleFullOutput,
    ) -> io::Result<()> {
        use std::fs::File;
        use std::io::BufWriter;

        let elab_dir = self.cache_dir.join("elab");
        std::fs::create_dir_all(&elab_dir)?;

        let key_hex = hex_string(full_key);
        let path = elab_dir.join(format!("{key_hex}.full.bin"));

        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, entry).map_err(|e| io::Error::other(e))?;

        if self.verbose {
            eprintln!(
                "[elab-cache-full] wrote: {} ({} defs)",
                &key_hex[..16],
                entry.defs.len()
            );
        }

        Ok(())
    }
}
