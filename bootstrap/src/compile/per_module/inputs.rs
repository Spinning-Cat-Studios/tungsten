//! Cross-module codegen inputs, gathered once before any unit compiles:
//! def info for declares, the mono pipeline, the shared poly term registry,
//! and per-unit referenced globals (ADR 8.5.26g, 10.5.26h). Extracted from
//! `mod.rs` to keep that file within the file-size limit.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use tungsten_bootstrap::driver::{self, ModuleCodegenUnit};

use super::codegen_unit_name;
use super::compilation::{
    build_cross_module_info, build_poly_term_registry, collect_referenced_globals,
    find_colliding_names, DefInfo,
};
use super::mono::{self, MonoOwnershipMap};
use crate::compile::{convert_adt_types_for_codegen, CompileFlags};

/// Cross-module inputs shared by every codegen unit, gathered once up front.
///
/// `pub(in crate::compile)` (not `pub(super)`) so `check_extern_map_ambiguity`
/// can reuse the exact codegen-input pipeline (ADR 12.7.26b D2).
pub(in crate::compile) struct CodegenInputs {
    pub(in crate::compile) all_defs_info: BTreeMap<String, DefInfo>,
    /// Names that collide across units and need scoping.
    pub(in crate::compile) collisions: HashSet<String>,
    /// Collision index (ADR 12.7.26a): bare LLVM name → the `all_defs_info`
    /// keys of every same-named def, in key sort order. Only colliding names
    /// (≥ 2 defs) have entries; used to resolve or reject colliding references.
    pub(in crate::compile) collision_index: BTreeMap<String, Vec<String>>,
    pub(in crate::compile) codegen_adt_types:
        HashMap<String, (Vec<String>, Vec<tungsten_codegen::CodegenConstructor>)>,
    pub(in crate::compile) mono_table: mono::MonoRequestTable,
    pub(in crate::compile) mono_map: MonoOwnershipMap,
    pub(in crate::compile) poly_term_registry: HashMap<String, tungsten_core::terms::Term>,
    pub(in crate::compile) referenced_globals: Vec<BTreeSet<String>>,
}

/// Build the cross-module info, run the mono pipeline, build the shared poly
/// term registry, and pre-compute per-unit referenced globals — the inputs
/// every codegen worker needs (ADR 8.5.26g, 10.5.26h). Emits `[perf]` timing
/// lines when `flags.verbose`.
pub(in crate::compile) fn gather_codegen_inputs(
    units: &[ModuleCodegenUnit],
    source_root: &Path,
    flags: &CompileFlags,
    project: &driver::ProjectOutput,
) -> Result<CodegenInputs, String> {
    // Build a map of all definitions across all modules for cross-module declares.
    // Key: composite "unit::def_name", Value: DefInfo with llvm_name, type, owner
    let info_start = std::time::Instant::now();
    let collisions = find_colliding_names(units);
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("build_cross_module_info").entered();
    let all_defs_info = build_cross_module_info(units, &collisions, source_root);
    let collision_index = build_collision_index(&all_defs_info, &collisions);
    #[cfg(feature = "profile")]
    drop(_span);
    let info_elapsed = info_start.elapsed();

    // Convert ADT/record types once (shared across modules)
    let codegen_adt_types = convert_adt_types_for_codegen(project.adt_types.clone());

    // Mono pipeline: discover → freeze → assign → validate (ADR 8.5.26g §2.1)
    let mono_start = std::time::Instant::now();
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("run_mono_pipeline").entered();
    let (mono_table, mono_map) = run_mono_pipeline(units, source_root, flags, project)?;
    #[cfg(feature = "profile")]
    drop(_span);
    let mono_elapsed = mono_start.elapsed();

    // ADR 10.5.26h §2.1: Build shared poly term registry once (not per-worker).
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("build_poly_registry").entered();
    let registry_start = std::time::Instant::now();
    let poly_term_registry = build_poly_term_registry(units, &collisions, source_root);
    let registry_elapsed = registry_start.elapsed();
    #[cfg(feature = "profile")]
    drop(_span);

    // ADR 10.5.26h §2.3: Pre-compute referenced globals per unit (not per-worker).
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("collect_ref_globals").entered();
    let globals_start = std::time::Instant::now();
    let referenced_globals: Vec<BTreeSet<String>> = units
        .iter()
        .map(|u| collect_referenced_globals(u))
        .collect();
    let globals_elapsed = globals_start.elapsed();
    #[cfg(feature = "profile")]
    drop(_span);

    // P0 instrumentation (ADR 9.5.26d §2.1, 10.5.26i §2.1)
    if flags.verbose {
        eprintln!(
            "[perf] {} codegen unit(s), {} cross-module info entries, {} job(s)",
            units.len(),
            all_defs_info.len(),
            flags.codegen_jobs
        );
        eprintln!(
            "[perf] cross-module info: {:.1?}, mono pipeline: {:.1?}",
            info_elapsed, mono_elapsed
        );
        eprintln!(
            "[perf] poly term registry: {} entries in {:.1?}",
            poly_term_registry.len(),
            registry_elapsed
        );
        eprintln!(
            "[perf] referenced globals: {} units in {:.1?}",
            referenced_globals.len(),
            globals_elapsed
        );
    }

    Ok(CodegenInputs {
        all_defs_info,
        collisions,
        collision_index,
        codegen_adt_types,
        mono_table,
        mono_map,
        poly_term_registry,
        referenced_globals,
    })
}

/// Index every colliding bare LLVM name to its same-named `all_defs_info`
/// keys, in key sort order (ADR 12.7.26a). One pass over `all_defs_info`
/// instead of a per-reference scan.
fn build_collision_index(
    all_defs_info: &BTreeMap<String, DefInfo>,
    collisions: &HashSet<String>,
) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for key in all_defs_info.keys() {
        let original = key.split("::").last().unwrap_or(key);
        let original_llvm = crate::compile::def_llvm_name(original);
        if collisions.contains(&original_llvm) {
            index.entry(original_llvm).or_default().push(key.clone());
        }
    }
    index
}

/// Run the mono pipeline: discover → freeze → assign owners → validate.
///
/// Returns the frozen request table and the ownership map. The table is needed
/// so per-unit compilation can look up which keys a unit requests; the map
/// provides ownership (define vs declare) decisions.
fn run_mono_pipeline(
    units: &[ModuleCodegenUnit],
    source_root: &Path,
    flags: &CompileFlags,
    project: &driver::ProjectOutput,
) -> Result<(mono::MonoRequestTable, MonoOwnershipMap), String> {
    let unit_names: Vec<String> = units
        .iter()
        .map(|u| codegen_unit_name(&u.source_file, source_root, &u.defs[0].name))
        .collect();

    let concrete_type_names = project.concrete_type_names();

    let mut mono_table = mono::discover_mono_requests(units, source_root, &concrete_type_names);
    if flags.diagnostics.tracing.trace_mono {
        eprintln!(
            "[mono] discovered {} request(s), {} unique key(s)",
            mono_table.requests().len(),
            mono_table.unique_keys().len()
        );
    }

    mono_table.freeze();
    let mono_map = mono::assign_owners(&mono_table, &unit_names);

    if flags.diagnostics.tracing.trace_mono {
        eprintln!("[mono] assigned {} ownership(s)", mono_map.len());
        for (key, ownership) in mono_map.entries() {
            eprintln!(
                "  {} → owner={}, symbol={}",
                key, ownership.owner_unit, ownership.symbol
            );
        }
    }

    mono::validate_symbols(&mono_map).map_err(|e| format!("mono symbol validation: {}", e))?;

    Ok((mono_table, mono_map))
}
