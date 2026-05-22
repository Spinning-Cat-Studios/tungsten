//! Internal pipeline execution functions.
//!
//! These functions handle the actual compilation pipeline steps:
//! building combined ASTs, elaboration with caching, evaluation, and sorry detection.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::ast::SourceFile;
use crate::cache::BuildCache;
use crate::elaborate::{collect_definitions, CoreDef, ElabError, ElabOutput, TypeProvenance};
use crate::{elaborate_with_warnings_full, parse};
use tungsten_core::{
    eval::{eval_with_env, EvalEnv},
    Context, Term, Type,
};

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
/// Used by Phase A.5 of per-module elaboration (ADR 5.5.26c) to produce
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
    let output = match elaborate_with_ir_cache(ast, source_path, opts.verbose, build, &trace) {
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

    // 2. Check for sorry
    let has_sorry = output.defs.iter().any(|d| d.term.contains_sorry());

    // 3. Evaluate if run mode, or return defs if test mode
    if opts.mode == Mode::Test {
        return Ok(PipelineResult::Tested {
            defs: output.defs,
            module_defs: module_ctx.module_defs,
            has_sorry,
        });
    }

    if opts.mode == Mode::Run {
        // Find main function
        if let Some(main_def) = output.defs.iter().find(|d| d.name == "main") {
            if opts.verbose {
                eprintln!("Evaluating main()...");
            }

            // Build globals map (excluding main) for environment-based evaluation.
            // This avoids exponential term blowup from naive substitution.
            let globals: HashMap<String, Term> = output
                .defs
                .iter()
                .filter(|d| d.name != "main")
                .map(|d| (d.name.clone(), d.term.term.clone()))
                .collect();

            let env = EvalEnv::new(globals);
            let value = eval_with_env(&main_def.term.term, &env);

            return Ok(PipelineResult::Evaluated {
                value,
                ty: main_def.ty.clone(),
            });
        }
        // Point to end of file since we don't have a better location
        let eof_span = crate::span::Span::new(source.len() as u32, source.len() as u32);
        let err = crate::ElabError::no_main_function(eof_span);
        render_diagnostics(source, &filename, &[err], &[]);
        return Ok(PipelineResult::Failed);
    }

    Ok(PipelineResult::Checked {
        num_defs: output.defs.len() + cached_def_count,
        has_sorry,
    })
}

/// Elaborate with IR caching using the hybrid approach.
///
/// The hybrid approach:
/// 1. Always run the collection pass (~10% of elaboration time)
/// 2. Compute types_hash from collected type definitions
/// 3. If cache hit: return cached CoreDefs (no warnings since we didn't elaborate)
/// 4. If cache miss: continue with elaboration and cache the result
///
/// Note: This is used for single-file runs (no module tree). Multi-file projects
/// use `per_module::elaborate_module_tree` instead (ADR 5.5.26c).
pub(super) fn elaborate_with_ir_cache(
    ast: &SourceFile,
    source_path: &Path,
    verbose: bool,
    build: &BuildCtx<'_>,
    trace: &TraceOptions,
) -> Result<ElabOutput, Vec<ElabError>> {
    use crate::elaborate::collect_definitions_with_modules;

    let mut ctx = Context::new();

    // If no cache, just elaborate directly
    let cache = match build.cache {
        Some(c) => c,
        None => {
            let output = if build.module_info.modules.is_empty() {
                let mut collected = collect_definitions(ast, &mut ctx)?;
                collected.apply_trace_options(trace);
                collected.elaborate()?
            } else {
                // With module info - use the module-aware collection
                let mut collected =
                    collect_definitions_with_modules(ast, &mut ctx, build.module_info.clone())?;
                collected.apply_trace_options(trace);
                collected.elaborate()?
            };
            return Ok(output);
        }
    };

    // Step 1: Run collection pass (always runs - ~10% of time)
    let collected = if build.module_info.modules.is_empty() {
        collect_definitions(ast, &mut ctx)?
    } else {
        collect_definitions_with_modules(ast, &mut ctx, build.module_info.clone())?
    };

    // Step 2: Compute types_hash from collected types
    let types = collected.types_for_hash();
    let types_hash = BuildCache::compute_types_hash(&types);

    // Step 3: Check IR cache
    if let Some(cached_defs) = cache.lock().unwrap().get_ir(source_path, &types_hash) {
        if verbose {
            eprintln!(
                "Using cached elaboration ({} definitions)",
                cached_defs.len()
            );
        }
        // Cache hit - return cached defs (no warnings since we didn't elaborate)
        // Note: record_types and adt_types are not cached, so we return empty for cached results
        // This is fine for non-compile use cases (check, run)
        return Ok(ElabOutput {
            defs: cached_defs,
            warnings: Vec::new(),
            record_types: HashMap::new(),
            adt_types: HashMap::new(),
            type_aliases: HashMap::new(),
            type_provenance: TypeProvenance::default(),
            encoded_types: HashMap::new(),
            mutual_recursion_groups: HashMap::new(),
            type_visibilities: HashMap::new(),
            record_field_visibilities: HashMap::new(),
        });
    }

    // Step 4: Cache miss - continue with elaboration
    let mut collected = collected;
    collected.apply_trace_options(trace);
    let output = collected.elaborate()?;

    // Step 5: Cache the result
    if let Err(e) = cache
        .lock()
        .unwrap()
        .put_ir(source_path, types_hash, &output.defs)
    {
        if verbose {
            eprintln!("[cache] warning: failed to cache IR: {e}");
        }
    }

    Ok(output)
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
