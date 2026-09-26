//! Driver module — orchestrates the compilation pipeline.
//!
//! This module ties together lexing, parsing, elaboration, type checking,
//! and evaluation into a cohesive pipeline.

use std::path::PathBuf;

mod cache;
pub(crate) use cache::cache_disabled_reason;
use cache::prepare_project_with_cache;

pub mod diagnostics;
mod error;
pub(crate) mod modules;
pub(crate) mod output;
pub(crate) mod per_module;
pub(crate) mod pipeline;
mod run_mode;
#[cfg(test)]
mod tests;
#[cfg(test)]
// Tests: tests_comparator_units.rs
#[path = "tests_comparator_units.rs"]
mod tests_comparator_units;
mod type_registry;

use diagnostics::set_max_errors;
pub use diagnostics::{render_diagnostics, render_diagnostics_with_source_map};
pub use error::PipelineError;
pub use modules::{
    build_module_info, get_module_name_from_parsed, parse_module_tree, resolve_pub_use_module,
    ModuleInfo, ParsedModule, SourceMap,
};
/// Re-exported from [`output`] — the complete result of project elaboration.
pub use output::{
    format_type, format_value, AdtTypes, ModuleCodegenUnit, PipelineOpts, ProjectOutput,
    RecordTypes, TraceOptions, TypeAliases, ValueImportTargetsByModule,
};
pub use per_module::cache::levels::{sort_submodules_by_deps, use_first_segments};
pub use per_module::inspect::{inspect_cache, CacheEntryKind, ModuleCacheRow};
pub use type_registry::{register_type_name, register_type_pattern, TypePattern};

// Re-export CoreDef for compile command
pub use crate::elaborate::CoreDef;

use modules::{build_source_map, extract_module_dependencies, flatten_module_tree};

use crate::cache::BuildCache;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tungsten_core::{Term, Type};

/// Result of running the compilation pipeline.
#[derive(Debug)]
pub enum PipelineResult {
    /// Successfully checked, with number of definitions.
    Checked {
        num_defs: usize,
        /// Holes by class over the definitions this run elaborated (ADR 18.9.26g).
        sorry: tungsten_core::terms::analysis::SorryCounts,
    },
    /// Successfully evaluated to a value.
    Evaluated { value: Term, ty: Type },
    /// Test run completed.
    Tested {
        defs: Vec<CoreDef>,
        /// Per-module definition groups for scoped test discovery (ADR 12.5.26b).
        /// Each entry is (module_path, source_file, defs).
        module_defs: Vec<(Vec<String>, std::path::PathBuf, Vec<CoreDef>)>,
        /// Type information for the evaluator's comparator-synthesis callback
        /// when running `test_*` bodies: record fields plus the μ-cluster member
        /// map a mutually recursive comparison needs (ADR 29.6.26f / T13;
        /// ADR 1.8.26b D2).
        comparator_types: crate::comparator::ComparatorTypes,
        sorry: tungsten_core::terms::analysis::SorryCounts,
    },
    /// Compilation failed.
    Failed,
}

/// Mode of operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Type-check only.
    Check,
    /// Type-check and evaluate main().
    Run,
    /// Discover and run test_* functions.
    Test,
}

/// Run the compilation pipeline on a source file.
///
/// This handles module resolution: if the file contains `mod foo;` declarations,
/// it will recursively parse and include those submodules.
pub fn run_file(path: &Path, mode: Mode, verbose: bool) -> Result<PipelineResult, PipelineError> {
    let opts = PipelineOpts {
        mode,
        verbose,
        dump_types: false,
    };
    run_file_with_options(path, &opts, false, 20)
}

/// Shared module tree preparation used by both `run_file_with_options` and `elaborate_project`.
pub(super) struct PreparedProject {
    pub(super) module_tree: ParsedModule,
    pub(super) source: String,
    pub(super) source_map: SourceMap,
    pub(super) module_info: ModuleInfo,
}

/// Parse module tree, discover siblings, check for parse errors, and build combined AST.
pub(super) fn prepare_project(
    path: &Path,
    verbose: bool,
    cache: Option<&Mutex<BuildCache>>,
) -> Result<PreparedProject, PipelineError> {
    // Reset the comparator request registry so stale entries from a prior
    // in-process invocation cannot leak (ADR 29.6.26f §T11.8). Shared by the
    // run/test (`run_file_with_options`) and compile (`elaborate_project`) paths.
    crate::comparator::requests::clear();

    // Parse the module tree (handles `mod foo;` declarations)
    // Use parallel pre-parsing when available (ADR 11.5.26b §P3)
    let mut visited = HashSet::new();
    let mut chain = Vec::new();

    let project_dir = path.parent().unwrap_or(Path::new("."));
    let tg_files = modules::parse::discover_tg_files(project_dir);
    let module_tree = if tg_files.len() > 1 {
        let preparsed = modules::parse::parse_files_parallel(&tg_files);
        modules::parse::parse_module_tree_with_preparsed(
            path,
            &mut visited,
            &mut chain,
            cache,
            &preparsed,
        )?
    } else {
        parse_module_tree(path, &mut visited, &mut chain, cache)?
    };

    // Discover and parse sibling modules for cross-module imports
    let workspace_root = modules::find_workspace_root(path);
    let sibling_modules = modules::parse_workspace_modules(&workspace_root, cache);
    let workspace_module_info = modules::build_workspace_module_info(&sibling_modules);

    if verbose && !sibling_modules.is_empty() {
        let sibling_names: Vec<_> = sibling_modules
            .iter()
            .map(|m| modules::get_module_name_from_parsed(m))
            .collect();
        eprintln!(
            "Discovered {} sibling module(s) at workspace root {}: {sibling_names:?}",
            sibling_modules.len(),
            workspace_root.display(),
        );
    }

    // Flatten all modules
    let all_items = flatten_module_tree(&module_tree);

    if verbose {
        eprintln!(
            "Parsed {} module(s) with {} total item(s)",
            pipeline::count_modules(&module_tree),
            all_items.len()
        );
    }

    // Read source for diagnostics
    let source = fs::read_to_string(path)
        .map_err(|e| PipelineError::IoError(path.display().to_string(), e.to_string()))?;

    // Build source map for multi-file error reporting
    let source_map = build_source_map(&module_tree);

    // Check for parse errors
    let mut source_map_vec = Vec::new();
    let parse_errors = modules::collect_parse_errors(&module_tree, &mut source_map_vec);
    if !parse_errors.is_empty() {
        for (file_path, errors) in &parse_errors {
            if let Some((_, src)) = source_map_vec.iter().find(|(p, _)| p == file_path) {
                render_diagnostics(src, &file_path.display().to_string(), &[], errors);
            }
        }
        return Err(PipelineError::ElabFailed("parse errors".to_string()));
    }

    // Build module info (no combined AST — per-module elaboration, ADR 5.5.26c)
    // Main module info is base (priority) so its module paths, use_statement
    // mappings, and file_to_module entries take precedence over workspace
    // sibling duplicates (ADR 8.5.26a).
    let file_module_info = build_module_info(&module_tree);
    let module_info = modules::merge_module_info(file_module_info, workspace_module_info);

    Ok(PreparedProject {
        module_tree,
        source,
        source_map,
        module_info,
    })
}

/// Run the compilation pipeline with additional options.
///
/// Like `run_file`, but allows disabling the cache and setting max errors.
///
/// Cache can be disabled via:
/// - `no_cache` parameter (from `--no-cache` CLI flag)
/// - `TUNGSTEN_NO_CACHE` environment variable (any non-empty value)
///
/// `max_errors` limits the number of errors displayed (0 = no limit).
pub fn run_file_with_options(
    path: &Path,
    opts: &PipelineOpts,
    no_cache: bool,
    max_errors: usize,
) -> Result<PipelineResult, PipelineError> {
    // Set max_errors for this run
    set_max_errors(max_errors);

    let (cache, prepared) = match prepare_project_with_cache(path, opts, no_cache) {
        Ok(result) => result,
        Err(PipelineError::ElabFailed(ref msg)) if msg == "parse errors" => {
            return Ok(PipelineResult::Failed);
        }
        Err(e) => return Err(e),
    };

    // Elaborate per-module with two-phase approach (ADR 5.5.26c)
    let elab_mode = match opts.mode {
        Mode::Test => crate::elaborate::ElabMode::Test,
        Mode::Run => crate::elaborate::ElabMode::Compile,
        Mode::Check => crate::elaborate::ElabMode::Check,
    };
    let trace = output::TraceOptions {
        elab_mode,
        ..output::TraceOptions::default()
    };
    let build = pipeline::BuildCtx {
        cache: cache.as_ref(),
        module_info: prepared.module_info,
        source_map: prepared.source_map,
    };

    let tree_output = match per_module::elaborate_module_tree(
        &prepared.module_tree,
        path,
        opts.verbose,
        &build,
        &trace,
    ) {
        Ok(t) => t,
        Err(elab_errors) => {
            let filename = path.to_string_lossy();
            render_diagnostics_with_source_map(
                &prepared.source,
                &filename,
                &build.source_map,
                &elab_errors,
                &[],
            );
            return Ok(PipelineResult::Failed);
        }
    };

    // Comparators for `__compare`-emitted `compare_T` globals are already in
    // `output.defs`: `run_module_tree` synthesizes them ahead of the termination
    // gate (ADR 11.8.26b §2.3), which is what the evaluator's environment is
    // built from. This function used to append them here, past the gate.
    let output = tree_output.elab;

    // Render any warnings (non-fatal)
    if !output.warnings.is_empty() {
        let filename = path.to_string_lossy();
        render_diagnostics_with_source_map(
            &prepared.source,
            &filename,
            &build.source_map,
            &[],
            &output.warnings,
        );
    }

    let module_ctx = pipeline::ModuleContext {
        cached_def_count: tree_output.cached_def_count,
        module_defs: tree_output.module_defs,
    };
    pipeline::run_with_output_cached_defs(output, &prepared.source, path, opts, module_ctx)
}

/// Build the final codegen units: per-module defs → units, with @-prefixed
/// TyVars stripped at the elab→codegen boundary (ADR 10.5.26d P7), plus
/// synthesized comparator units appended (ADR 29.6.26f §T11.8).
fn finalize_codegen_units(
    module_defs: Vec<(Vec<String>, PathBuf, Vec<CoreDef>)>,
    verbose: bool,
    types: &crate::comparator::ComparatorTypes,
) -> Vec<output::ModuleCodegenUnit> {
    let mut units: Vec<_> = output::build_codegen_units(module_defs)
        .into_iter()
        .map(|mut unit| {
            unit.defs = unit
                .defs
                .into_iter()
                .map(|d| d.strip_at_prefixes())
                .collect();
            unit
        })
        .collect();
    // Synthesized terms are already @-free, so they are appended after stripping.
    let synth = synthesized_comparator_units(units.iter().flat_map(|u| &u.defs), verbose, types);
    units.extend(synth);
    units
}

/// Build synthetic codegen units for `__compare`-emitted `compare_T` globals that
/// lack a definition (ADR 29.6.26f §T11.8) — the *concrete* `__compare` path, which
/// references comparators directly as `Global("compare_T")`.
///
/// The *generic* `compare` path (`TyApp(Global("__cmp"), T)`) is resolved separately,
/// at codegen time, by the comparator-synthesis intercept (P6′ step 2,
/// `CodeGen::compile_comparator_intrinsic`) — not here, because those instances only
/// become concrete after monomorphization.
///
/// Each synthesized comparator becomes its own unit (per-unit "exactly one def" convention).
fn synthesized_comparator_units<'a>(
    defs: impl IntoIterator<Item = &'a CoreDef>,
    verbose: bool,
    types: &crate::comparator::ComparatorTypes,
) -> Vec<output::ModuleCodegenUnit> {
    crate::comparator::discover::synth_missing_comparators(defs, types)
        .into_iter()
        .map(|def| {
            if verbose {
                eprintln!("Synthesized comparator `{}`", def.name);
            }
            output::ModuleCodegenUnit {
                module_path: vec!["__comparator".to_string()],
                source_file: std::path::PathBuf::from("<synthesized>"),
                defs: vec![def],
            }
        })
        .collect()
}

/// The compile path's cache handle: no caching by default — codegen needs
/// full CoreDef bodies, which the signature-only elab cache (ADR 10.5.26n)
/// intentionally omits. Exception: TUNGSTEN_ELAB_CACHE_FULL=1 enables the
/// full-output cache (ADR 12.5.26a), which does carry bodies.
fn full_output_cache_for_compile(path: &Path, verbose: bool) -> Option<Mutex<BuildCache>> {
    let full_output_cache = std::env::var("TUNGSTEN_ELAB_CACHE_FULL")
        .map(|v| v == "1")
        .unwrap_or(false);
    if !full_output_cache {
        return None;
    }
    let project_root = path.parent().unwrap_or(Path::new("."));
    match BuildCache::new(project_root, verbose) {
        Ok(c) => Some(Mutex::new(c)),
        Err(e) => {
            if verbose {
                eprintln!("[cache] warning: failed to initialize cache: {e}");
            }
            None
        }
    }
}

/// Multi-module project elaboration + the live-normalization inspector variant.
mod project;
pub use project::{elaborate_project, elaborate_project_with_inspector, ProjectNormalizer};

/// Run the compilation pipeline on source code (single file, no module resolution).
pub use pipeline::{check_verdict_line, run_source};

/// Run the pipeline on a single expression (for eval command).
pub use pipeline::eval_expr;
