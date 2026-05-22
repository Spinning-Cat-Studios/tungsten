//! Per-module codegen: emit separate files per codegen unit (ADR 6.5.26c §2.3).
//!
//! Each `ModuleCodegenUnit` is compiled into its own LLVM module.
//! Cross-module references are emitted as `declare` (external) declarations.
//! By default, each unit emits a `.o` object file directly via in-process LLVM
//! (ADR 9.5.26e §2.1). With `--emit-llvm`, `.ll` text files are written instead.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use tungsten_bootstrap::driver::{self, ModuleCodegenUnit};
use tungsten_codegen::inkwell::context::Context as LlvmContext;
use tungsten_codegen::CodeGen;

use super::mono::{self, MonoOwnershipMap};
use super::{convert_adt_types_for_codegen, CompileFlags};

mod compilation;
mod entry;
mod prelude;
mod unit_compile;

#[allow(unused_imports)] // used by tests via `use super::*`
pub(super) use entry::resolve_emit_llvm_dir;
pub(super) use entry::run_codegen_per_module;

use unit_compile::{compile_single_unit, OutputConfig};

#[allow(unused_imports)] // used by tests via `use super::*`
use compilation::scoped_llvm_name;
use compilation::{
    build_cross_module_info, build_poly_term_registry, collect_referenced_globals,
    find_colliding_names, DefInfo,
};

/// Output format of a compiled module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputKind {
    /// LLVM IR text (`.ll`)
    Ll,
    /// Native object file (`.o`)
    Obj,
}

/// Compiled module ready for linking.
pub(super) struct CompiledModule {
    /// Path to the emitted file (`.ll` or `.o`)
    pub(super) output_path: PathBuf,
    /// Module name (for diagnostics)
    pub(super) name: String,
    /// What kind of output file this is
    pub(super) kind: OutputKind,
}

/// Run per-module codegen: compile each codegen unit into a separate file.
///
/// When `--emit-llvm` is set, emits `.ll` text files. Otherwise, emits `.o`
/// object files directly from in-memory LLVM modules (ADR 9.5.26e §2.1).
///
/// Returns the list of compiled module paths on success, or an error message.
pub(super) fn run_per_module_codegen(
    file: &PathBuf,
    flags: &CompileFlags,
    project: &driver::ProjectOutput,
    main_ty: &tungsten_core::types::Type,
    output_dir: &Path,
) -> Result<Vec<CompiledModule>, String> {
    let units = &project.codegen_units;

    // Source root is the entry file's parent directory (ADR 7.5.26h §2.3)
    let source_root = file.parent().unwrap_or(Path::new(".")).to_path_buf();

    log_unit_listing(units, &source_root, flags);

    // Build a map of all definitions across all modules for cross-module declares.
    // Key: composite "unit::def_name", Value: DefInfo with llvm_name, type, owner
    let info_start = std::time::Instant::now();
    let collisions = find_colliding_names(units);
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("build_cross_module_info").entered();
    let all_defs_info = build_cross_module_info(units, &collisions, &source_root);
    #[cfg(feature = "profile")]
    drop(_span);
    let info_elapsed = info_start.elapsed();

    // Convert ADT/record types once (shared across modules)
    let codegen_adt_types = convert_adt_types_for_codegen(project.adt_types.clone());

    // Mono pipeline: discover → freeze → assign → validate (ADR 8.5.26g §2.1)
    let mono_start = std::time::Instant::now();
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("run_mono_pipeline").entered();
    let (mono_table, mono_map) = run_mono_pipeline(units, &source_root, flags, project)?;
    #[cfg(feature = "profile")]
    drop(_span);
    let mono_elapsed = mono_start.elapsed();

    // ADR 10.5.26h §2.1: Build shared poly term registry once (not per-worker).
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("build_poly_registry").entered();
    let registry_start = std::time::Instant::now();
    let poly_term_registry = build_poly_term_registry(units, &collisions, &source_root);
    let registry_elapsed = registry_start.elapsed();
    #[cfg(feature = "profile")]
    drop(_span);

    // ADR 10.5.26h §2.3: Pre-compute referenced globals per unit (not per-worker).
    #[cfg(feature = "profile")]
    let _span = tracing::info_span!("collect_ref_globals").entered();
    let globals_start = std::time::Instant::now();
    let referenced_globals: Vec<HashSet<String>> = units
        .iter()
        .map(|u| collect_referenced_globals(u))
        .collect();
    let globals_elapsed = globals_start.elapsed();
    #[cfg(feature = "profile")]
    drop(_span);

    let ctx = UnitCompileCtx {
        all_defs_info: &all_defs_info,
        collisions: &collisions,
        project: ProjectCtx {
            record_types: &project.record_types,
            adt_types: &codegen_adt_types,
            main_ty,
            file,
            source_root: &source_root,
        },
        flags,
        mono: MonoCtx {
            map: &mono_map,
            table: &mono_table,
        },
        poly_term_registry: &poly_term_registry,
        referenced_globals: &referenced_globals,
    };

    let emit_obj = !flags.emit_llvm;
    let codegen_jobs = flags.codegen_jobs;

    // P0 instrumentation (ADR 9.5.26d §2.1, 10.5.26i §2.1)
    if flags.verbose {
        eprintln!(
            "[perf] {} codegen unit(s), {} cross-module info entries, {} job(s)",
            units.len(),
            all_defs_info.len(),
            codegen_jobs
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
    let codegen_start = std::time::Instant::now();

    // P3: Parallel codegen — each worker creates its own LlvmContext (ADR 9.5.26e §P3).
    let compiled = if codegen_jobs == 1 {
        compile_units_sequential(units, &ctx, output_dir, emit_obj)?
    } else {
        compile_units_parallel(units, &ctx, output_dir, emit_obj, codegen_jobs)?
    };

    if flags.verbose {
        let label = if emit_obj {
            "Stage 1 codegen"
        } else {
            "Stage 1 IR gen"
        };
        eprintln!(
            "[perf] {}: {:.1}s ({} unit(s) + depot)",
            label,
            codegen_start.elapsed().as_secs_f64(),
            units.len()
        );
    }

    Ok(compiled)
}

/// Sequential codegen path — single LlvmContext, no thread overhead (ADR 9.5.26e §P3).
fn compile_units_sequential(
    units: &[ModuleCodegenUnit],
    ctx: &UnitCompileCtx<'_>,
    output_dir: &Path,
    emit_obj: bool,
) -> Result<Vec<CompiledModule>, String> {
    let llvm_context = LlvmContext::create();
    let mut compiled = Vec::new();
    for (i, unit) in units.iter().enumerate() {
        let unit_name = codegen_unit_name(
            &unit.source_file,
            ctx.project.source_root,
            &unit.defs[0].name,
        );
        let (ext, kind) = if emit_obj {
            ("o", OutputKind::Obj)
        } else {
            ("ll", OutputKind::Ll)
        };
        // Index prefix prevents case-insensitive filesystem collisions
        // (e.g., char_A.o vs char_a.o on macOS APFS).
        let output_path = output_dir.join(format!("{}_{}.{}", i, unit_name, ext));

        if ctx.flags.verbose {
            eprintln!("Compiling module '{}'...", unit_name);
        }

        let output_cfg = OutputConfig {
            path: &output_path,
            emit_obj,
        };
        compile_single_unit(
            &llvm_context,
            unit,
            ctx,
            &output_cfg,
            &ctx.referenced_globals[i],
        )?;

        compiled.push(CompiledModule {
            output_path,
            name: unit_name,
            kind,
        });
    }
    // __mono depot unit (ADR 9.5.26b §2.3)
    prelude::compile_mono_depot(&llvm_context, ctx, output_dir, emit_obj, &mut compiled)?;
    Ok(compiled)
}

/// Parallel codegen path — bounded work-stealing via std::thread::scope (ADR 9.5.26e §P3).
/// Each worker creates its own LlvmContext. UnitCompileCtx is naturally Sync.
fn compile_units_parallel(
    units: &[ModuleCodegenUnit],
    ctx: &UnitCompileCtx<'_>,
    output_dir: &Path,
    emit_obj: bool,
    codegen_jobs: usize,
) -> Result<Vec<CompiledModule>, String> {
    use std::sync::Mutex;

    let work_queue: Mutex<std::iter::Enumerate<std::slice::Iter<'_, ModuleCodegenUnit>>> =
        Mutex::new(units.iter().enumerate());

    let results: Vec<Result<CompiledModule, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..codegen_jobs.min(units.len()))
            .map(|_| {
                let work_queue = &work_queue;
                let ctx = &ctx;
                s.spawn(move || {
                    let thread_ctx = LlvmContext::create();
                    let mut local_results = Vec::new();
                    loop {
                        let item = { work_queue.lock().unwrap().next() };
                        let Some((idx, unit)) = item else { break };
                        let name = codegen_unit_name(
                            &unit.source_file,
                            ctx.project.source_root,
                            &unit.defs[0].name,
                        );
                        let (ext, kind) = if emit_obj {
                            ("o", OutputKind::Obj)
                        } else {
                            ("ll", OutputKind::Ll)
                        };
                        // Index prefix prevents case-insensitive filesystem collisions
                        // (e.g., char_A.o vs char_a.o on macOS APFS).
                        let path = output_dir.join(format!("{}_{}.{}", idx, name, ext));
                        if ctx.flags.verbose {
                            eprintln!("Compiling module '{}'...", name);
                        }
                        let output_cfg = OutputConfig {
                            path: &path,
                            emit_obj,
                        };
                        let r = compile_single_unit(
                            &thread_ctx,
                            unit,
                            ctx,
                            &output_cfg,
                            &ctx.referenced_globals[idx],
                        );
                        local_results.push(r.map(|()| CompiledModule {
                            output_path: path,
                            name,
                            kind,
                        }));
                    }
                    local_results
                })
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });

    // Collect results, propagating errors
    let mut compiled = Vec::with_capacity(results.len());
    let mut errors = Vec::new();
    for r in results {
        match r {
            Ok(m) => compiled.push(m),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    // Sort for deterministic link order regardless of completion order
    compiled.sort_by(|a, b| a.name.cmp(&b.name));

    // __mono depot unit — sequential, uses its own context (ADR 9.5.26b §2.3)
    let mono_context = LlvmContext::create();
    prelude::compile_mono_depot(&mono_context, ctx, output_dir, emit_obj, &mut compiled)?;
    Ok(compiled)
}

/// Log per-unit listing when verbose mode is on.
fn log_unit_listing(units: &[ModuleCodegenUnit], source_root: &Path, flags: &CompileFlags) {
    if flags.verbose {
        eprintln!(
            "Per-module codegen: {} codegen unit(s) (source_root={})",
            units.len(),
            source_root.display(),
        );
        for unit in units {
            let name = codegen_unit_name(&unit.source_file, source_root, &unit.defs[0].name);
            eprintln!(
                "  - {} ({} defs, {})",
                name,
                unit.defs.len(),
                unit.source_file.display()
            );
        }
    }
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

/// Mono pipeline context (ADR 8.5.26g): ownership map + request table.
pub(super) struct MonoCtx<'a> {
    /// Single-owner monomorphization map
    pub(super) map: &'a MonoOwnershipMap,
    /// Frozen mono request table — used to find which keys a unit requests
    pub(super) table: &'a mono::MonoRequestTable,
}

/// Project-level data shared across all codegen units.
pub(super) struct ProjectCtx<'a> {
    pub(super) record_types: &'a driver::RecordTypes,
    pub(super) adt_types:
        &'a HashMap<String, (Vec<String>, Vec<tungsten_codegen::CodegenConstructor>)>,
    pub(super) main_ty: &'a tungsten_core::types::Type,
    /// Entry file path (for debug info)
    pub(super) file: &'a PathBuf,
    /// Source root for deriving codegen unit names (ADR 7.5.26h)
    pub(super) source_root: &'a Path,
}

/// Shared context for compiling codegen units.
pub(super) struct UnitCompileCtx<'a> {
    pub(super) all_defs_info: &'a BTreeMap<String, DefInfo>,
    /// Names that collide across units and need scoping
    pub(super) collisions: &'a HashSet<String>,
    pub(super) project: ProjectCtx<'a>,
    pub(super) flags: &'a CompileFlags,
    /// Single-owner monomorphization context (ADR 8.5.26g)
    pub(super) mono: MonoCtx<'a>,
    /// Shared poly term registry — built once, cloned into each worker (ADR 10.5.26h §2.1)
    pub(super) poly_term_registry: &'a HashMap<String, tungsten_core::terms::Term>,
    /// Pre-computed referenced globals per unit (ADR 10.5.26h §2.3)
    pub(super) referenced_globals: &'a [HashSet<String>],
}

/// Derive the codegen unit name for a per-function unit (ADR 9.5.26b).
///
/// Produces a deterministic name like `elab__env__defs__lookup_type` from
/// the source file path relative to the source root plus the def name.
pub(crate) fn codegen_unit_name(source_file: &Path, source_root: &Path, def_name: &str) -> String {
    let base = file_unit_base(source_file, source_root);
    let llvm_name = super::def_llvm_name(def_name);
    format!("{}__{}", base, llvm_name)
}

/// Derive the file-level prefix from a source file path (ADR 7.5.26h).
///
/// Produces `elab__env__defs` from the source file path relative to the source root.
/// Used for directory structure in `--emit-llvm` and as the base for per-function
/// unit names.
pub(crate) fn file_unit_base(source_file: &Path, source_root: &Path) -> String {
    let relative = source_file.strip_prefix(source_root).unwrap_or(source_file);
    relative
        .with_extension("")
        .to_string_lossy()
        .replace('/', "__")
}

#[cfg(test)]
mod tests;
