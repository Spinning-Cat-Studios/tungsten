//! Per-module codegen: emit separate files per codegen unit (ADR 6.5.26c §2.3).
//!
//! Each `ModuleCodegenUnit` is compiled into its own LLVM module.
//! Cross-module references are emitted as `declare` (external) declarations.
//! By default, each unit emits a `.o` object file directly via in-process LLVM
//! (ADR 9.5.26e §2.1). With `--emit-llvm`, `.ll` text files are written instead.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use tungsten_bootstrap::driver;
#[allow(unused_imports)] // used by tests via `use crate::compile::per_module::*`
use tungsten_bootstrap::driver::ModuleCodegenUnit;

use super::mono::{self, MonoOwnershipMap};
use super::CompileFlags;

// `compilation`, `depot`, and `inputs` are `pub(in crate::compile)` so
// `check_extern_map_ambiguity` can call the same resolution functions real
// codegen uses (ADR 12.7.26b D2) — never re-derive them.
pub(in crate::compile) mod compilation;
pub(in crate::compile) mod depot;
mod drivers;
// One derivation of the --emit-llvm destination, shared with
// `info codegen unit-paths` (ADR 28.7.26e retrospective).
pub(crate) mod emit_paths;
mod entry;
pub(in crate::compile) mod imports;
pub(in crate::compile) mod inputs;
mod unit_compile;
mod unit_selection;

use drivers::{compile_units_parallel, compile_units_sequential};
use inputs::gather_codegen_inputs;
use tungsten_codegen::inkwell::context::Context as LlvmContext;

#[allow(unused_imports)] // used by tests via `use super::*`
pub(super) use emit_paths::resolve_emit_llvm_dir;
pub(super) use entry::run_codegen_per_module;

#[allow(unused_imports)] // used by tests via `use super::*`
use compilation::scoped_llvm_name;
use compilation::DefInfo;

/// Where per-module codegen sends its output: the file directory and an
/// optional structured musttail-decision sink (ADR 1.7.26b). Bundling these
/// keeps `run_per_module_codegen` within the parameter-count limit; the sink is
/// `None` for normal compiles (zero overhead).
pub(super) struct CodegenOutput<'a> {
    /// Directory for emitted `.ll` / `.o` files.
    pub(super) dir: &'a Path,
    /// Optional musttail-decision sink drained per unit (diagnostics only).
    pub(super) musttail_sink: Option<&'a std::sync::Mutex<Vec<tungsten_codegen::MusttailDecision>>>,
    /// Optional per-unit cost sink (ADR 8.7.26a): wall time + allocation
    /// volume per unit, fed to `doctor check unit-cost`. `None` for normal
    /// compiles (zero overhead).
    pub(super) unit_cost_sink:
        Option<&'a std::sync::Mutex<Vec<tungsten_core::diagnostics::unit_cost::UnitCostRecord>>>,
}

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
    output: CodegenOutput<'_>,
) -> Result<Vec<CompiledModule>, String> {
    let output_dir = output.dir;
    let musttail_sink = output.musttail_sink;
    let unit_cost_sink = output.unit_cost_sink;
    let units = &project.codegen_units;

    // Source root is the entry file's parent directory (ADR 7.5.26h §2.3)
    let source_root = file.parent().unwrap_or(Path::new(".")).to_path_buf();

    // Cross-module analysis: def info, mono pipeline, poly registry, ref globals.
    // Always gathered from the FULL unit set — even under --only-unit — so
    // declares and mono ownership match a full build (ADR 3.7.26b).
    let inputs = gather_codegen_inputs(units, &source_root, flags, project)?;

    // Pair each unit with its derived name + referenced globals, then
    // resolve --only-unit isolation and serial scheduling (ADR 3.7.26b).
    let work =
        unit_selection::pair_units_with_globals(units, &source_root, &inputs.referenced_globals);
    log_unit_listing(&work, &source_root, flags);
    let plan = unit_selection::plan_unit_schedule(work, flags)?;

    let comparator_types = tungsten_bootstrap::comparator::ComparatorTypes::new(
        project.record_types.clone(),
        &project.encoded_types,
        &project.type_provenance,
        project.adt_types.clone(),
        &project.mutual_recursion_groups,
    );
    let ctx = UnitCompileCtx {
        all_defs_info: &inputs.all_defs_info,
        collisions: &inputs.collisions,
        collision_index: &inputs.collision_index,
        import_targets: &project.value_import_targets,
        project: ProjectCtx {
            comparator_types: &comparator_types,
            adt_types: &inputs.codegen_adt_types,
            main_ty,
            file,
            source_root: &source_root,
        },
        flags,
        mono: MonoCtx {
            map: &inputs.mono_map,
            table: &inputs.mono_table,
        },
        poly_term_registry: &inputs.poly_term_registry,
        musttail_sink,
        unit_cost_sink,
        total_units: plan.work.len(),
    };

    let emit_obj = !flags.emit_llvm;
    let codegen_start = std::time::Instant::now();

    // P3: Parallel codegen — each worker creates its own LlvmContext (ADR 9.5.26e §P3).
    let emit_target = drivers::EmitTarget {
        output_dir,
        emit_obj,
    };
    let mut compiled = if flags.codegen_jobs == 1 {
        compile_units_sequential(&plan.work, &ctx, emit_target)?
    } else {
        compile_units_parallel(
            &plan.work,
            &ctx,
            emit_target,
            flags.codegen_jobs,
            plan.schedule,
        )?
    };

    // __mono depot unit (ADR 9.5.26b §2.3) — skipped under --only-unit: the
    // depot aggregates specializations from the whole program, which would
    // defeat single-unit isolation (ADR 3.7.26b).
    if plan.compile_depot {
        let mono_context = LlvmContext::create();
        depot::compile_mono_depot(&mono_context, &ctx, output_dir, emit_obj, &mut compiled)?;
    }

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
            plan.work.len()
        );
    }

    Ok(compiled)
}

/// Log per-unit listing when verbose mode is on.
fn log_unit_listing(
    work: &[unit_selection::UnitWork<'_>],
    source_root: &Path,
    flags: &CompileFlags,
) {
    if flags.verbose {
        eprintln!(
            "Per-module codegen: {} codegen unit(s) (source_root={})",
            work.len(),
            source_root.display(),
        );
        for item in work {
            eprintln!(
                "  - {} ({} defs, {})",
                item.unit_name,
                item.unit.defs.len(),
                item.unit.source_file.display()
            );
        }
    }
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
    /// Type information for comparator synthesis, which owns the record map —
    /// carrying both would let the two drift (ADR 1.8.26b D2).
    pub(super) comparator_types: &'a tungsten_bootstrap::comparator::ComparatorTypes,
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
    /// Collision index: bare LLVM name → same-named `all_defs_info` keys
    /// (ADR 12.7.26a) — resolves or rejects colliding references.
    pub(super) collision_index: &'a BTreeMap<String, Vec<String>>,
    /// Per-module value import targets from elaboration (ADR 12.7.26a §2.1).
    pub(super) import_targets: &'a driver::ValueImportTargetsByModule,
    pub(super) project: ProjectCtx<'a>,
    pub(super) flags: &'a CompileFlags,
    /// Single-owner monomorphization context (ADR 8.5.26g)
    pub(super) mono: MonoCtx<'a>,
    /// Shared poly term registry — built once, cloned into each worker (ADR 10.5.26h §2.1)
    pub(super) poly_term_registry: &'a HashMap<String, tungsten_core::terms::Term>,
    /// Optional musttail-decision sink (ADR 1.7.26b). When `Some`, each unit's
    /// structured musttail decisions are drained here after compilation. `None`
    /// for normal compiles (zero overhead). The `Mutex` makes it `Sync` for the
    /// parallel codegen path.
    pub(super) musttail_sink: Option<&'a std::sync::Mutex<Vec<tungsten_codegen::MusttailDecision>>>,
    /// Optional per-unit cost sink (ADR 8.7.26a) — see [`CodegenOutput`].
    pub(super) unit_cost_sink:
        Option<&'a std::sync::Mutex<Vec<tungsten_core::diagnostics::unit_cost::UnitCostRecord>>>,
    /// Total units in this codegen run — the `N` of the `[i/N]` verbose
    /// progress tag (ADR 8.7.26a §2.4).
    pub(super) total_units: usize,
}

impl<'a> UnitCompileCtx<'a> {
    /// The borrowed lookup tables colliding-reference resolution needs
    /// (ADR 12.7.26a).
    pub(super) fn collision_context(&self) -> imports::CollisionContext<'a> {
        imports::CollisionContext {
            collision_index: self.collision_index,
            all_defs_info: self.all_defs_info,
            import_targets: self.import_targets,
        }
    }
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
