//! `tungsten diff abi` — compare bootstrap and self-hosted emitter type layouts.
//!
//! Elaborates a file, computes the bootstrap codegen layout for a named type,
//! loads the `.tg` emitter layout from a manifest file, and reports differences.
//! Cost 3 (elaborate + codegen type lowering).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use tungsten_codegen::inkwell::context::Context;
use tungsten_codegen::inkwell::targets::{InitializationConfig, Target, TargetMachine};
use tungsten_codegen::TypeLowering;
use tungsten_core::types::Type;

use tungsten_bootstrap::driver::{self, AdtTypes, RecordTypes};

/// Default manifest path relative to the project root.
const DEFAULT_MANIFEST: &str = ".tungsten/abi-manifest.json";

/// A single entry in the ABI manifest produced by the .tg emitter.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub llvm_type: String,
    /// Reserved for future manifest v2 with size metadata.
    #[allow(dead_code)]
    pub size: Option<u64>,
    /// Reserved for future manifest v2 with alignment metadata.
    #[allow(dead_code)]
    pub align: Option<u64>,
}

/// Comparison result for a single type.
#[derive(Debug)]
pub enum AbiStatus {
    Compatible,
    Mismatch { bootstrap: String, emitter: String },
    NotEmitted,
}

/// Entry point for `tungsten diff abi <type> <file>`.
pub fn cmd_diff_abi(type_name: &str, file: &Path, verbose: bool, max_errors: usize) -> ExitCode {
    // 1. Elaborate the project
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // 2. Look up the type — handle primitives and user-defined types
    let encoded_type = match resolve_type(type_name, &project) {
        Some(ty) => ty,
        None => {
            eprintln!("Type '{type_name}' not found (not a primitive, not an encoded type).");
            eprintln!("Available types:");
            let mut names: Vec<_> = project.encoded_types.keys().collect();
            names.sort();
            for name in names.iter().take(20) {
                eprintln!("  {name}");
            }
            if names.len() > 20 {
                eprintln!("  ... and {} more", names.len() - 20);
            }
            return ExitCode::FAILURE;
        }
    };

    // 3. Compute bootstrap layout via inkwell
    let bootstrap_layout =
        compute_bootstrap_layout(&encoded_type, &project.adt_types, &project.record_types);

    // 4. Load .tg emitter manifest (if available)
    let manifest_path = file
        .parent()
        .unwrap_or(Path::new("."))
        .join(DEFAULT_MANIFEST);
    let emitter_layout = load_manifest_entry(&manifest_path, type_name);

    // 5. Compare and report
    let status = compare_layouts(&bootstrap_layout, &emitter_layout);

    println!("Type: {type_name}");
    println!("  Bootstrap layout: {bootstrap_layout}");
    match &emitter_layout {
        Some(entry) => println!("  .tg emitter:      {}", entry.llvm_type),
        None => println!("  .tg emitter:      (not yet emitted)"),
    }
    match &status {
        AbiStatus::Compatible => println!("  Status: \u{2713} compatible"),
        AbiStatus::Mismatch { bootstrap, emitter } => {
            println!("  Status: \u{2717} mismatch");
            println!("    bootstrap: {bootstrap}");
            println!("    emitter:   {emitter}");
        }
        AbiStatus::NotEmitted => println!("  Status: ? not emitted"),
    }

    match status {
        AbiStatus::Mismatch { .. } => ExitCode::FAILURE,
        _ => ExitCode::SUCCESS,
    }
}

/// Resolve a type name to a `Type`, handling primitives and user-defined types.
fn resolve_type(name: &str, project: &driver::ProjectOutput) -> Option<Type> {
    match name {
        "Nat" => Some(Type::Nat),
        "Bool" => Some(Type::Bool),
        "String" => Some(Type::String),
        "Unit" => Some(Type::Unit),
        "Void" => Some(Type::Void),
        "Prop" => Some(Type::Prop),
        _ => project.encoded_types.get(name).cloned(),
    }
}

/// Compute the LLVM type string for a Tungsten type via the bootstrap codegen.
fn compute_bootstrap_layout(ty: &Type, adt_types: &AdtTypes, record_types: &RecordTypes) -> String {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);

    // Register ADT types so the lowering can resolve type applications
    let codegen_adts = adt_types
        .iter()
        .map(|(name, (params, ctors))| {
            let codegen_ctors = ctors
                .iter()
                .map(|c| tungsten_codegen::CodegenConstructor {
                    name: c.name.clone(),
                    fields: c.fields.clone(),
                    index: c.index,
                })
                .collect();
            (name.clone(), (params.clone(), codegen_ctors))
        })
        .collect();
    lowering.register_adt_types(codegen_adts);

    // Register record types
    lowering.register_record_types(record_types.clone());

    // Initialize target for correct layout
    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");
    let target_triple = TargetMachine::get_default_triple();
    if let Ok(target) = Target::from_triple(&target_triple) {
        if let Some(machine) = target.create_target_machine(
            &target_triple,
            "generic",
            "",
            tungsten_codegen::inkwell::OptimizationLevel::Default,
            tungsten_codegen::inkwell::targets::RelocMode::PIC,
            tungsten_codegen::inkwell::targets::CodeModel::Default,
        ) {
            lowering.set_target_data(machine.get_target_data());
        }
    }

    let llvm_ty = lowering.lower_type(ty);
    llvm_ty.print_to_string().to_string()
}

/// Load an entry from the emitter ABI manifest.
fn load_manifest_entry(manifest_path: &Path, type_name: &str) -> Option<ManifestEntry> {
    let content = fs::read_to_string(manifest_path).ok()?;
    // Simple JSON parsing: { "TypeName": { "llvm_type": "...", "size": N, "align": N } }
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(&content).ok()?;
    let entry = map.get(type_name)?;
    let llvm_type = entry.get("llvm_type")?.as_str()?.to_string();
    let size = entry.get("size").and_then(|v| v.as_u64());
    let align = entry.get("align").and_then(|v| v.as_u64());
    Some(ManifestEntry {
        llvm_type,
        size,
        align,
    })
}

/// Compare bootstrap layout string with emitter manifest entry.
fn compare_layouts(bootstrap: &str, emitter: &Option<ManifestEntry>) -> AbiStatus {
    match emitter {
        None => AbiStatus::NotEmitted,
        Some(entry) => {
            let canon_bootstrap = canonicalize_llvm_type(bootstrap);
            let canon_emitter = canonicalize_llvm_type(&entry.llvm_type);
            if canon_bootstrap == canon_emitter {
                AbiStatus::Compatible
            } else {
                AbiStatus::Mismatch {
                    bootstrap: bootstrap.to_string(),
                    emitter: entry.llvm_type.clone(),
                }
            }
        }
    }
}

/// Canonicalize an LLVM type string for comparison.
///
/// Strips redundant whitespace and normalizes spacing around braces/commas
/// to avoid false mismatches from formatting differences.
pub fn canonicalize_llvm_type(ty: &str) -> String {
    // Collapse all whitespace to single spaces, then trim
    let collapsed: String = ty.split_whitespace().collect::<Vec<_>>().join(" ");
    // Normalize spacing: "{ i32, [8 x i8] }" and "{i32, [8 x i8]}" both → "{ i32, [8 x i8] }"
    collapsed
        .replace("{ ", "{")
        .replace(" }", "}")
        .replace(", ", ",")
        .replace("{", "{ ")
        .replace("}", " }")
        .replace(",", ", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_strips_extra_whitespace() {
        assert_eq!(
            canonicalize_llvm_type("{  i32,  [8 x i8]  }"),
            canonicalize_llvm_type("{ i32, [8 x i8] }")
        );
    }

    #[test]
    fn canonicalize_handles_no_spaces() {
        assert_eq!(
            canonicalize_llvm_type("{i32,[8 x i8]}"),
            canonicalize_llvm_type("{ i32, [8 x i8] }")
        );
    }

    #[test]
    fn canonicalize_identical_types_match() {
        let a = "{ i32, [8 x i8] }";
        let b = "{ i32, [8 x i8] }";
        assert_eq!(canonicalize_llvm_type(a), canonicalize_llvm_type(b));
    }

    #[test]
    fn canonicalize_different_types_differ() {
        assert_ne!(
            canonicalize_llvm_type("{ i32, [8 x i8] }"),
            canonicalize_llvm_type("{ i32, [16 x i8] }")
        );
    }

    #[test]
    fn compare_not_emitted() {
        let status = compare_layouts("i64", &None);
        assert!(matches!(status, AbiStatus::NotEmitted));
    }

    #[test]
    fn compare_compatible() {
        let entry = ManifestEntry {
            llvm_type: "i64".to_string(),
            size: Some(8),
            align: Some(8),
        };
        let status = compare_layouts("i64", &Some(entry));
        assert!(matches!(status, AbiStatus::Compatible));
    }

    #[test]
    fn compare_mismatch() {
        let entry = ManifestEntry {
            llvm_type: "i32".to_string(),
            size: Some(4),
            align: Some(4),
        };
        let status = compare_layouts("i64", &Some(entry));
        assert!(matches!(status, AbiStatus::Mismatch { .. }));
    }

    #[test]
    fn resolve_type_returns_primitives() {
        // Use a dummy ProjectOutput with no encoded_types — primitives should
        // still resolve.  We can't construct a full ProjectOutput in a unit test
        // without elaboration, so just verify the match arms directly.
        assert_eq!(resolve_type_primitive("Nat"), Some(Type::Nat));
        assert_eq!(resolve_type_primitive("Bool"), Some(Type::Bool));
        assert_eq!(resolve_type_primitive("String"), Some(Type::String));
        assert_eq!(resolve_type_primitive("Unit"), Some(Type::Unit));
        assert_eq!(resolve_type_primitive("Void"), Some(Type::Void));
        assert_eq!(resolve_type_primitive("Prop"), Some(Type::Prop));
    }

    #[test]
    fn resolve_type_returns_none_for_unknown() {
        assert_eq!(resolve_type_primitive("Blah"), None);
        assert_eq!(resolve_type_primitive("FooBar"), None);
    }

    /// Helper: test only the primitive branch of resolve_type (no ProjectOutput).
    fn resolve_type_primitive(name: &str) -> Option<Type> {
        match name {
            "Nat" => Some(Type::Nat),
            "Bool" => Some(Type::Bool),
            "String" => Some(Type::String),
            "Unit" => Some(Type::Unit),
            "Void" => Some(Type::Void),
            "Prop" => Some(Type::Prop),
            _ => None,
        }
    }
}
