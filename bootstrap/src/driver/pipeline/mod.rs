//! Internal pipeline execution functions.
//!
//! These functions handle the actual compilation pipeline steps:
//! building combined ASTs, elaboration with caching, evaluation, and sorry detection.

pub(super) mod ir_cached_elab;
#[cfg(test)]
mod verdict_tests;

use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::ast::SourceFile;
use crate::cache::BuildCache;
use crate::elaborate::{collect_definitions, CoreDef, ElabError, ElabOutput, TypeProvenance};
use crate::{elaborate_with_warnings_full, parse};
use tungsten_core::terms::analysis::SorryCounts;
use tungsten_core::Context;

use super::modules::{self, ModuleInfo, ParsedModule, SourceMap};
use super::output::{format_type, PipelineOpts, TraceOptions};
use super::{
    render_diagnostics, render_diagnostics_with_source_map, Mode, PipelineError, PipelineResult,
};
use crate::elaborate::ElabMode;

/// Build context: cache, module info, and source map for elaboration.
pub(super) struct BuildCtx<'a> {
    pub cache: Option<&'a Mutex<BuildCache>>,
    pub module_info: ModuleInfo,
    pub source_map: SourceMap,
}

/// Count total modules in a tree.
pub(super) fn count_modules(module: &ParsedModule) -> usize {
    1 + module.submodules.iter().map(count_modules).sum::<usize>()
}

/// Build a combined SourceFile from all modules in the tree, along with
/// a mapping from item indices to their source files for provenance tracking.
///
/// Used by Signature Collection of per-module elaboration (ADR 5.5.26c) to produce
/// a single SourceFile for the global collection pass.
///
/// Submodules are processed first so their definitions are available to the parent.
/// The index_to_file mapping allows disambiguation when different files have
/// items at the same byte offsets - each item has a unique index in the combined AST.
pub(crate) fn build_combined_ast(module: &ParsedModule) -> (SourceFile, Vec<PathBuf>) {
    use crate::ast::Item;

    let mut items = Vec::new();
    let mut index_to_file: Vec<PathBuf> = Vec::new();

    // First, recursively add items from submodules
    // This ensures child definitions are available to the parent
    for submodule in &module.submodules {
        let (sub_ast, sub_index_to_file) = build_combined_ast(submodule);
        items.extend(sub_ast.items);
        index_to_file.extend(sub_index_to_file);
    }

    // Then add items from this module (excluding mod declarations)
    for item in &module.source_file.items {
        if matches!(item, Item::Mod(_)) {
            continue;
        }

        // Track which file this item came from using its index
        items.push(item.clone());
        index_to_file.push(module.path.clone());
    }

    (
        SourceFile {
            items,
            span: module.source_file.span,
        },
        index_to_file,
    )
}

/// Run the compilation pipeline on source code (single file, no module resolution).
pub fn run_source(
    source: &str,
    filename: &str,
    mode: Mode,
    verbose: bool,
) -> Result<PipelineResult, PipelineError> {
    // 1. Parse
    let (ast, parse_errors) = parse(source);

    if !parse_errors.is_empty() {
        render_diagnostics(source, filename, &[], &parse_errors);
        return Ok(PipelineResult::Failed);
    }

    if verbose {
        eprintln!("Parsed {} item(s)", ast.items.len());
    }

    // No cache for direct source runs (e.g., eval, tests)
    let opts = PipelineOpts {
        mode,
        verbose,
        dump_types: false,
    };
    run_ast(&ast, source, Path::new(filename), &opts, None)
}

/// Run the pipeline on an already-parsed AST.
fn run_ast(
    ast: &SourceFile,
    source: &str,
    source_path: &Path,
    opts: &PipelineOpts,
    cache: Option<&Mutex<BuildCache>>,
) -> Result<PipelineResult, PipelineError> {
    // No module info for single-file runs
    let build = BuildCtx {
        cache,
        module_info: ModuleInfo::default(),
        source_map: SourceMap::single(source_path.to_path_buf(), source.to_string()),
    };
    run_ast_with_modules(ast, source, source_path, opts, &build)
}

/// Run the pipeline on an already-parsed AST with module info.
pub(super) fn run_ast_with_modules(
    ast: &SourceFile,
    source: &str,
    source_path: &Path,
    opts: &PipelineOpts,
    build: &BuildCtx<'_>,
) -> Result<PipelineResult, PipelineError> {
    let filename = source_path.to_string_lossy();

    // 1. Elaborate (Surface AST → Core) with IR caching
    let elab_mode = match opts.mode {
        Mode::Test => ElabMode::Test,
        Mode::Run => ElabMode::Compile,
        Mode::Check => ElabMode::Check,
    };
    let trace = TraceOptions {
        elab_mode,
        ..TraceOptions::default()
    };
    let output = match ir_cached_elab::elaborate_with_ir_cache(
        ast,
        source_path,
        opts.verbose,
        build,
        &trace,
    ) {
        Ok(output) => output,
        Err(elab_errors) => {
            render_diagnostics_with_source_map(
                source,
                &filename,
                &build.source_map,
                &elab_errors,
                &[],
            );
            return Ok(PipelineResult::Failed);
        }
    };

    // Render any warnings (non-fatal)
    if !output.warnings.is_empty() {
        render_diagnostics_with_source_map(
            source,
            &filename,
            &build.source_map,
            &[],
            &output.warnings,
        );
    }

    run_with_output(output, source, source_path, opts)
}

/// Context from per-module elaboration that needs to flow into the pipeline (ADR 12.5.26b).
pub(super) struct ModuleContext {
    /// Number of definitions loaded from cache (not freshly elaborated).
    pub cached_def_count: usize,
    /// Per-module definition groups for scoped test discovery.
    pub module_defs: Vec<(Vec<String>, std::path::PathBuf, Vec<CoreDef>)>,
}

/// Post-elaboration pipeline: sorry check, eval, test, or check (ADR 5.5.26c §2.3).
///
/// Shared by both `run_ast_with_modules` (single-file) and `run_file_with_options`
/// (per-module). Takes an already-elaborated `ElabOutput` and runs the mode-specific
/// pipeline steps. Warnings should already be rendered by the caller.
pub(super) fn run_with_output(
    output: ElabOutput,
    source: &str,
    source_path: &Path,
    opts: &PipelineOpts,
) -> Result<PipelineResult, PipelineError> {
    let ctx = ModuleContext {
        cached_def_count: 0,
        module_defs: Vec::new(),
    };
    run_with_output_cached_defs(output, source, source_path, opts, ctx)
}

/// Like `run_with_output` but includes module context from per-module elaboration.
pub(super) fn run_with_output_cached_defs(
    output: ElabOutput,
    source: &str,
    source_path: &Path,
    opts: &PipelineOpts,
    module_ctx: ModuleContext,
) -> Result<PipelineResult, PipelineError> {
    let filename = source_path.to_string_lossy();
    let cached_def_count = module_ctx.cached_def_count;

    if opts.verbose {
        let total = output.defs.len() + cached_def_count;
        if cached_def_count > 0 {
            eprintln!(
                "Elaborated {} definition(s) ({} fresh, {} cached)",
                total,
                output.defs.len(),
                cached_def_count
            );
        } else {
            eprintln!("Elaborated {} definition(s)", total);
        }
    }

    if opts.dump_types {
        for def in &output.defs {
            eprintln!("  {} : {}", def.name, format_type(&def.ty));
        }
    }

    // 2. Count holes by class (ADR 18.9.26g). Same population the bit read:
    // on a warm cache a hit module's defs are absent, so this is the hint and
    // `doctor check sorry-sites` is the census.
    let sorry = SorryCounts::of(output.defs.iter().map(|d| &*d.term));

    // 3. Evaluate if run mode, or return defs if test mode
    if opts.mode == Mode::Test {
        let comparator_types = crate::comparator::ComparatorTypes::new(
            output.record_types,
            &output.encoded_types,
            &output.type_provenance,
            output.adt_types,
            &output.mutual_recursion_groups,
        );
        return Ok(PipelineResult::Tested {
            defs: output.defs,
            module_defs: module_ctx.module_defs,
            comparator_types,
            sorry,
        });
    }

    if opts.mode == Mode::Run {
        // Evaluation + reporting live in `run_mode.rs` (kept out of this file
        // for the size limit).
        return Ok(super::run_mode::evaluate_main(&output, source, &filename));
    }

    Ok(PipelineResult::Checked {
        num_defs: output.defs.len() + cached_def_count,
        sorry,
    })
}

/// Run the pipeline on a single expression (for eval command).
pub fn eval_expr(
    source: &str,
    verbose: bool,
    _max_errors: usize,
) -> Result<PipelineResult, PipelineError> {
    // Wrap expression in a main function
    // Try common types since we don't have full type inference
    let attempts = [
        format!("fn main() -> Nat {{ {} }}", source),
        format!("fn main() -> Bool {{ {} }}", source),
        format!("fn main() -> Unit {{ {} }}", source),
    ];

    for attempt in &attempts {
        if let Ok(PipelineResult::Evaluated { value, ty }) =
            run_source(attempt, "<eval>", Mode::Run, verbose)
        {
            return Ok(PipelineResult::Evaluated { value, ty });
        }
    }

    // If all fail, show error from first attempt
    run_source(&attempts[0], "<eval>", Mode::Run, verbose)
}

/// The line `tungsten check` prints on success (ADR 18.9.26g).
///
/// A hole the author did not write — synthesised or unclassified — counts as
/// synthesised here; `doctor check sorry-sites` keeps the two apart. The
/// substring `contains sorry` is kept so anything grepping for it still matches.
#[must_use]
pub fn check_verdict_line(file: &str, num_defs: usize, sorry: &SorryCounts) -> String {
    if sorry.total() == 0 {
        return format!("✓ {file}: {num_defs} definition(s), all OK");
    }
    format!(
        "⚠ {file}: {num_defs} definition(s), contains sorry ({} authored, {} synthesised)",
        sorry.authored,
        sorry.not_authored()
    )
}
