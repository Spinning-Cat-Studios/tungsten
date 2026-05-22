//! Tests for full-output elaboration cache (ADR 12.5.26a).

use super::*;
use crate::cache::build::BuildCache;
use crate::elaborate::{CoreDef, ModuleExports, TypeProvenance};
fn dummy_core_def(name: &str) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty: tungsten_core::Type::Nat,
        term: tungsten_core::SpannedTerm {
            term: tungsten_core::Term::Var(name.to_string()),
            span: None,
        },
        span: crate::span::Span::default(),
    }
}

fn sample_full_output() -> CachedModuleFullOutput {
    CachedModuleFullOutput {
        defs: vec![dummy_core_def("f"), dummy_core_def("g")],
        record_types: std::collections::HashMap::new(),
        adt_types: std::collections::HashMap::new(),
        type_aliases: std::collections::HashMap::new(),
        type_provenance: TypeProvenance::default(),
        encoded_types: std::collections::HashMap::new(),
        mutual_recursion_groups: std::collections::HashMap::new(),
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
    }
}

// --- Key computation tests ---

#[test]
fn full_output_key_is_deterministic() {
    let sig_key = [0xAA; 32];
    let k1 = compute_full_output_cache_key(&sig_key, 1);
    let k2 = compute_full_output_cache_key(&sig_key, 1);
    assert_eq!(k1, k2);
}

#[test]
fn full_output_key_changes_with_schema_version() {
    let sig_key = [0xAA; 32];
    let k1 = compute_full_output_cache_key(&sig_key, 1);
    let k2 = compute_full_output_cache_key(&sig_key, 2);
    assert_ne!(k1, k2);
}

#[test]
fn full_output_key_differs_from_signature_key() {
    let sig_key = [0xAA; 32];
    let full_key = compute_full_output_cache_key(&sig_key, FULL_OUTPUT_SCHEMA_VERSION);
    assert_ne!(sig_key, full_key);
}

// --- Round-trip tests ---

#[test]
fn full_output_roundtrip_via_bincode() {
    let entry = sample_full_output();
    let bytes = bincode::serialize(&entry).unwrap();
    let restored: CachedModuleFullOutput = bincode::deserialize(&bytes).unwrap();
    assert_eq!(restored.defs.len(), 2);
    assert_eq!(restored.defs[0].name, "f");
    assert_eq!(restored.defs[1].name, "g");
}

#[test]
fn full_output_put_then_get_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let key = [0xBB; 32];
    let entry = sample_full_output();

    cache.put_module_full_output(&key, &entry).unwrap();
    let loaded = cache
        .get_module_full_output(&key)
        .expect("should hit after put");
    assert_eq!(loaded.defs.len(), 2);
    assert_eq!(loaded.defs[0].name, "f");
    assert_eq!(loaded.defs[1].name, "g");
}

#[test]
fn full_output_into_elab_output_preserves_defs() {
    let entry = sample_full_output();
    let output = entry.into_elab_output();
    assert_eq!(output.defs.len(), 2);
    assert_eq!(output.defs[0].name, "f");
    assert!(output.record_types.is_empty());
}

// --- Corrupt entry fallback tests ---

#[test]
fn full_output_corrupt_entry_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let key = [0xCC; 32];

    // Write garbage bytes to the full.bin file
    let elab_dir = dir.path().join(".tungsten").join("cache").join("elab");
    std::fs::create_dir_all(&elab_dir).unwrap();
    let key_hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let path = elab_dir.join(format!("{key_hex}.full.bin"));
    std::fs::write(&path, b"this is not valid bincode").unwrap();

    // Should return None (graceful fallback) and remove corrupt file
    assert!(cache.get_module_full_output(&key).is_none());
    assert!(!path.exists(), "corrupt file should be removed");
}

#[test]
fn full_output_truncated_entry_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let key = [0xDD; 32];

    // Write a valid entry, then truncate it
    let entry = sample_full_output();
    cache.put_module_full_output(&key, &entry).unwrap();

    let elab_dir = dir.path().join(".tungsten").join("cache").join("elab");
    let key_hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let path = elab_dir.join(format!("{key_hex}.full.bin"));
    let full_bytes = std::fs::read(&path).unwrap();
    // Truncate to half
    std::fs::write(&path, &full_bytes[..full_bytes.len() / 2]).unwrap();

    assert!(cache.get_module_full_output(&key).is_none());
}

// --- Isolation tests ---

#[test]
fn full_output_miss_returns_none_for_nonexistent_key() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    assert!(cache.get_module_full_output(&[0xFF; 32]).is_none());
}

#[test]
fn full_output_does_not_interfere_with_signature_cache() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let sig_key = [0xAA; 32];
    let full_key = compute_full_output_cache_key(&sig_key, FULL_OUTPUT_SCHEMA_VERSION);

    // Write a signature entry
    let sig = CachedModuleSignature {
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
        def_count: 3,
    };
    cache.put_module_elab(&sig_key, &sig).unwrap();

    // Write a full-output entry with same base key
    let entry = sample_full_output();
    cache.put_module_full_output(&full_key, &entry).unwrap();

    // Both should be independently retrievable
    let loaded_sig = cache.get_module_elab(&sig_key).expect("sig should hit");
    assert_eq!(loaded_sig.def_count, 3);
    let loaded_full = cache
        .get_module_full_output(&full_key)
        .expect("full should hit");
    assert_eq!(loaded_full.defs.len(), 2);
}

// --- Gating tests ---

#[test]
fn full_output_not_written_when_only_signature_written() {
    // Verifies that writing a signature entry does NOT produce a .full.bin file.
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let sig_key = [0xEE; 32];
    let sig = CachedModuleSignature {
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
        def_count: 5,
    };
    cache.put_module_elab(&sig_key, &sig).unwrap();

    // No .full.bin should exist
    let elab_dir = dir.path().join(".tungsten").join("cache").join("elab");
    let full_files: Vec<_> = std::fs::read_dir(&elab_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "bin")
                .unwrap_or(false)
        })
        .filter(|e| e.file_name().to_string_lossy().contains(".full."))
        .collect();
    assert!(
        full_files.is_empty(),
        "no .full.bin should exist when only signature is written"
    );
}

#[test]
fn full_output_from_output_captures_warnings() {
    use crate::elaborate::ElabOutput;
    use crate::ElabErrorKind;
    let mut output = ElabOutput {
        defs: vec![dummy_core_def("h")],
        warnings: Vec::new(),
        record_types: std::collections::HashMap::new(),
        adt_types: std::collections::HashMap::new(),
        type_aliases: std::collections::HashMap::new(),
        type_provenance: TypeProvenance::default(),
        encoded_types: std::collections::HashMap::new(),
        mutual_recursion_groups: std::collections::HashMap::new(),
        type_visibilities: std::collections::HashMap::new(),
        record_field_visibilities: std::collections::HashMap::new(),
    };
    output.warnings.push(crate::elaborate::ElabError {
        message: "unused variable".to_string(),
        span: crate::span::Span::default(),
        kind: ElabErrorKind::UndefinedVariable("x".to_string()),
        notes: Vec::new(),
        help: None,
        context: None,
        file_path: None,
        trace: Vec::new(),
    });
    let exports = ModuleExports::default();
    let cached = CachedModuleFullOutput::from_output(&output, &exports, &exports);
    assert_eq!(cached.warnings.len(), 1);
    assert_eq!(cached.warnings[0].message, "unused variable");

    // Round-trip preserves warnings
    let restored = cached.into_elab_output();
    assert_eq!(restored.warnings.len(), 1);
    assert_eq!(restored.warnings[0].message, "unused variable");
}

#[test]
fn full_output_write_failure_is_non_fatal() {
    // If the cache directory is read-only, put_module_full_output returns Err
    // but must not panic.
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();

    // Create elab dir, then make it read-only
    let elab_dir = dir.path().join(".tungsten").join("cache").join("elab");
    std::fs::create_dir_all(&elab_dir).unwrap();
    let mut perms = std::fs::metadata(&elab_dir).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&elab_dir, perms.clone()).unwrap();

    let key = [0xFF; 32];
    let entry = sample_full_output();
    let result = cache.put_module_full_output(&key, &entry);
    // Should be an error, not a panic
    assert!(result.is_err());

    // Restore permissions for cleanup
    perms.set_readonly(false);
    std::fs::set_permissions(&elab_dir, perms).unwrap();
}

// --- Stats tests ---

#[test]
fn elab_cache_stats_includes_full_output() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();

    let sig = CachedModuleSignature {
        delta_exports: ModuleExports::default(),
        warnings: Vec::new(),
        def_count: 0,
    };
    cache.put_module_elab(&[0x01; 32], &sig).unwrap();

    let entry = sample_full_output();
    let full_key = compute_full_output_cache_key(&[0x01; 32], FULL_OUTPUT_SCHEMA_VERSION);
    cache.put_module_full_output(&full_key, &entry).unwrap();

    let stats = cache.elab_cache_stats().unwrap();
    assert_eq!(stats.entry_count, 1);
    assert_eq!(stats.full_output_count, 1);
    assert!(stats.full_output_bytes > 0);
}

// --- Serialize + background writer integration tests (ADR 10.5.26o) ---

#[test]
fn serialize_full_output_entry_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let full_key = [0xAB; 32];
    let entry = sample_full_output();

    // Serialize (optionally compressed)
    let (path, bytes) = cache
        .serialize_full_output_entry(&full_key, &entry)
        .unwrap();
    assert!(!bytes.is_empty());

    // Write to disk (simulates what background writer does)
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &bytes).unwrap();

    // Read back via get_module_full_output
    let loaded = cache
        .get_module_full_output(&full_key)
        .expect("should load serialized entry");
    assert_eq!(loaded.defs.len(), 2);
    assert_eq!(loaded.defs[0].name, "f");
    assert_eq!(loaded.defs[1].name, "g");
}

#[test]
fn background_writer_writes_serialized_entries() {
    use crate::cache::elab_cache::writer::BackgroundWriter;
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let full_key = [0xCD; 32];
    let entry = sample_full_output();

    let (path, bytes) = cache
        .serialize_full_output_entry(&full_key, &entry)
        .unwrap();

    // Use background writer
    let writer = BackgroundWriter::spawn(4);
    writer.send(path.clone(), bytes);
    let errors = writer.join();
    assert!(errors.is_empty(), "expected no write errors: {errors:?}");

    // Verify the entry was written and can be read back
    assert!(path.exists());
    let loaded = cache
        .get_module_full_output(&full_key)
        .expect("should read bg-written entry");
    assert_eq!(loaded.defs.len(), 2);
}

#[test]
#[cfg(not(feature = "compress"))]
fn zst_entry_returns_none_without_compress_feature() {
    let dir = tempfile::tempdir().unwrap();
    let cache = BuildCache::new(dir.path(), false).unwrap();
    let full_key = [0xEE; 32];

    // Manually write a .full.bin.zst file (simulates a compressed entry left by
    // a build with the compress feature enabled)
    let elab_dir = dir.path().join("elab");
    std::fs::create_dir_all(&elab_dir).unwrap();
    let key_hex: String = full_key.iter().map(|b| format!("{b:02x}")).collect();
    let zst_path = elab_dir.join(format!("{key_hex}.full.bin.zst"));
    std::fs::write(&zst_path, b"fake compressed data").unwrap();

    // Without compress feature, .zst entries are unreadable — should return None
    let result = cache.get_module_full_output(&full_key);
    assert!(result.is_none());
}
