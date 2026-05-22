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
#[cfg(test)]
mod tests;
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
    RecordTypes, TraceOptions, TypeAliases,
};
pub use per_module::cache::levels::{sort_submodules_by_deps, use_first_segments};
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
    Checked { num_defs: usize, has_sorry: bool },
    /// Successfully evaluated to a value.
    Evaluated { value: Term, ty: Type },
    /// Test run completed.
    Tested {
        defs: Vec<CoreDef>,
        /// Per-module definition groups for scoped test discovery (ADR 12.5.26b).
        /// Each entry is (module_path, source_file, defs).
        module_defs: Vec<(Vec<String>, std::path::PathBuf, Vec<CoreDef>)>,
        has_sorry: bool,
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

/// Elaborate a multi-module project, returning the compiled definitions.
///
/// This is the entry point for the `compile` command. Unlike `run_file_with_options`,
/// this returns the elaborated definitions instead of checking/evaluating them,
/// so they can be passed to codegen.
///
/// Returns:
/// - `Ok((defs, record_types, adt_types, source_map))` on success, where `defs` are the elaborated definitions,
///   `record_types` maps record names to their fields for codegen, `adt_types` maps ADT names to their
///   constructors for Type::App expansion, and `source_map` maps file paths to source code for error reporting
/// - `Err(PipelineError)` on failure
pub fn elaborate_project(
    path: &Path,
    verbose: bool,
    max_errors: usize,
    trace: Option<&TraceOptions>,
) -> Result<ProjectOutput, PipelineError> {
    // Set max_errors for this run
    set_max_errors(max_errors);

    // No caching for compile mode — codegen needs full CoreDef bodies, which the
    // signature-only elab cache (ADR 10.5.26n) intentionally omits. Cache is only
    // useful for `check` mode where we verify types without emitting code.
    // Exception: when TUNGSTEN_ELAB_CACHE_FULL=1 is set, the full-output cache
    // (ADR 12.5.26a) provides CoreDef bodies, enabling cache hits in compile mode.
    let full_output_cache = std::env::var("TUNGSTEN_ELAB_CACHE_FULL")
        .map(|v| v == "1")
        .unwrap_or(false);
    let cache: Option<Mutex<BuildCache>> = if full_output_cache {
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
    } else {
        None
    };

    // Parse and prepare the project
    let prepared = prepare_project(path, verbose, cache.as_ref())?;

    // Elaborate per-module with two-phase approach (ADR 5.5.26c)
    let trace_opts = trace.cloned().unwrap_or_default();
    let build = pipeline::BuildCtx {
        cache: cache.as_ref(),
        module_info: prepared.module_info,
        source_map: prepared.source_map,
    };
    let tree_output = match per_module::elaborate_module_tree(
        &prepared.module_tree,
        path,
        verbose,
        &build,
        &trace_opts,
    ) {
        Ok(output) => output,
        Err(elab_errors) => {
            let filename = path.to_string_lossy();
            render_diagnostics_with_source_map(
                &prepared.source,
                &filename,
                &build.source_map,
                &elab_errors,
                &[],
            );
            return Err(PipelineError::ElabFailed("elaboration errors".to_string()));
        }
    };

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

    if verbose {
        eprintln!("Elaborated {} definition(s)", output.defs.len());
    }

    // Build codegen units from per-module defs (ADR 6.5.26c §2.2)
    let codegen_units = output::build_codegen_units(tree_output.module_defs);

    // Strip @-prefixed TyVars at the elaboration→codegen boundary (ADR 10.5.26d P7).
    // These are an elaboration-internal convention that must not leak downstream.
    let defs: Vec<_> = output
        .defs
        .into_iter()
        .map(|d| d.strip_at_prefixes())
        .collect();
    let codegen_units: Vec<_> = codegen_units
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

    Ok(ProjectOutput {
        defs,
        codegen_units,
        record_types: output.record_types,
        adt_types: output.adt_types,
        type_aliases: output.type_aliases,
        type_provenance: output.type_provenance,
        source_map: build.source_map,
        encoded_types: output.encoded_types,
        mutual_recursion_groups: output.mutual_recursion_groups,
        type_visibilities: output.type_visibilities,
        record_field_visibilities: output.record_field_visibilities,
    })
}

/// Run the compilation pipeline on source code (single file, no module resolution).
pub use pipeline::run_source;

/// Run the pipeline on a single expression (for eval command).
pub use pipeline::eval_expr;
