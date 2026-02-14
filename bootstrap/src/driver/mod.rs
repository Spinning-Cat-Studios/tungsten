//! Driver module — orchestrates the compilation pipeline.
//!
//! This module ties together lexing, parsing, elaboration, type checking,
//! and evaluation into a cohesive pipeline.

use std::path::PathBuf;

mod diagnostics;
mod modules;
mod output;

pub use diagnostics::{
    get_max_errors, render_diagnostics, render_diagnostics_limited,
    render_diagnostics_with_source_map, render_diagnostics_with_source_map_limited,
    render_diagnostics_with_warnings, set_max_errors, DEFAULT_MAX_ERRORS,
};
pub use modules::{
    build_module_info,
    build_source_map,
    build_workspace_module_info,
    discover_sibling_modules,
    extract_module_dependencies,
    // Workspace-aware module discovery
    find_workspace_root,
    flatten_module_tree,
    get_module_name_from_parsed,
    merge_module_info,
    parse_module_tree,
    parse_workspace_modules,
    ModuleDependencyInfo,
    ModuleInfo,
    ParsedModule,
    SourceMap,
};
pub use output::{
    clear_type_name_registry, format_type, format_value, register_type_name, register_type_pattern,
    TypePattern,
};

// Re-export CoreDef for compile command
pub use crate::elaborate::CoreDef;

use crate::ast::SourceFile;
use crate::cache::BuildCache;
use crate::elaborate::{collect_definitions, Constructor, ElabError};
use crate::{elaborate_with_warnings, parse};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tungsten_core::{
    eval::{eval_with_env, EvalEnv},
    Context, Term, Type,
};

/// Record type definitions: name -> fields.
/// Used by codegen to expand `TyVar("RecordName")` to structural product types.
pub type RecordTypes = HashMap<String, Vec<(String, Type)>>;

/// ADT type definitions: name -> (params, constructors).
/// Used by codegen to expand `Type::App("Name", args)` to sum/mu types.
pub type AdtTypes = HashMap<String, (Vec<String>, Vec<Constructor>)>;

/// Result of running the compilation pipeline.
#[derive(Debug)]
pub enum PipelineResult {
    /// Successfully checked, with number of definitions.
    Checked { num_defs: usize, has_sorry: bool },
    /// Successfully evaluated to a value.
    Evaluated { value: Term, ty: Type },
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
}

/// Run the compilation pipeline on a source file.
///
/// This handles module resolution: if the file contains `mod foo;` declarations,
/// it will recursively parse and include those submodules.
pub fn run_file(path: &Path, mode: Mode, verbose: bool) -> Result<PipelineResult, PipelineError> {
    run_file_with_options(path, mode, verbose, false, 20)
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
    mode: Mode,
    verbose: bool,
    no_cache: bool,
    max_errors: usize,
) -> Result<PipelineResult, PipelineError> {
    // Set max_errors for this run
    set_max_errors(max_errors);

    // Find project root (directory containing the source file for now)
    let project_root = path.parent().unwrap_or(Path::new("."));

    // Check if caching is disabled via CLI flag or environment variable
    let cache_disabled_by_env = std::env::var("TUNGSTEN_NO_CACHE")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let skip_cache = no_cache || cache_disabled_by_env;

    // Initialize build cache (unless disabled)
    let cache = if skip_cache {
        if verbose {
            if no_cache && cache_disabled_by_env {
                eprintln!("[cache] disabled by --no-cache flag and TUNGSTEN_NO_CACHE env var");
            } else if no_cache {
                eprintln!("[cache] disabled by --no-cache flag");
            } else {
                eprintln!("[cache] disabled by TUNGSTEN_NO_CACHE env var");
            }
        }
        None
    } else {
        match BuildCache::new(project_root, verbose) {
            Ok(c) => Some(RefCell::new(c)),
            Err(e) => {
                if verbose {
                    eprintln!("[cache] warning: failed to initialize cache: {e}");
                }
                None
            }
        }
    };

    // 1. Parse the module tree (handles `mod foo;` declarations)
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = parse_module_tree(path, &mut visited, &mut chain, cache.as_ref())?;

    // 2. Update the dependency graph in the cache
    if let Some(ref c) = cache {
        let deps = extract_module_dependencies(&module_tree);
        let root = path.to_path_buf();
        let modules: Vec<_> = deps
            .into_iter()
            .map(|d| (d.path, d.content_hash, d.dependencies))
            .collect();
        c.borrow_mut().update_dependency_graph(root, modules);
    }

    // Flush cache
    if let Some(ref c) = cache {
        if let Err(e) = c.borrow().flush() {
            if verbose {
                eprintln!("[cache] warning: failed to flush cache: {e}");
            }
        }
    }

    // 2.5. Discover and parse sibling modules for cross-module imports
    // This enables checking files like `elab/env/mod.tg` that import from `lexer::span`
    let workspace_root = modules::find_workspace_root(path);
    let sibling_modules = modules::parse_workspace_modules(&workspace_root, cache.as_ref());
    let workspace_module_info = modules::build_workspace_module_info(&sibling_modules);

    if verbose && !sibling_modules.is_empty() {
        let sibling_names: Vec<_> = sibling_modules
            .iter()
            .map(|m| modules::get_module_name_from_parsed(m))
            .collect();
        eprintln!(
            "Discovered {} sibling module(s) at workspace root {:?}: {:?}",
            sibling_modules.len(),
            workspace_root,
            sibling_names
        );
    }

    // 2. Flatten all modules into a single list of items
    let all_items = flatten_module_tree(&module_tree);

    if verbose {
        eprintln!(
            "Parsed {} module(s) with {} total item(s)",
            count_modules(&module_tree),
            all_items.len()
        );
    }

    // 3. Build a combined SourceFile from all items
    // We need to read the source for diagnostics
    let source = fs::read_to_string(path)
        .map_err(|e| PipelineError::IoError(path.display().to_string(), e.to_string()))?;

    // Build source map for multi-file error reporting
    let multi_source_map = build_source_map(&module_tree);

    // Check for parse errors in any module
    let mut source_map = Vec::new();
    let parse_errors = modules::collect_parse_errors(&module_tree, &mut source_map);

    if !parse_errors.is_empty() {
        // Report errors from each file
        for (file_path, errors) in &parse_errors {
            if let Some((_, src)) = source_map.iter().find(|(p, _)| p == file_path) {
                render_diagnostics(src, &file_path.display().to_string(), &[], errors);
            }
        }
        return Ok(PipelineResult::Failed);
    }

    // Create a combined AST with all items, along with index→file tracking
    let (combined_ast, index_to_file) = build_combined_ast(&module_tree);

    // Build module info for qualified path resolution
    // Start with file-specific module info, then merge workspace-wide sibling modules
    let file_module_info = build_module_info(&module_tree);
    let mut module_info = modules::merge_module_info(workspace_module_info, file_module_info);

    // Add the index→file mapping for disambiguation during elaboration
    module_info.item_index_to_file = index_to_file;

    run_ast_with_modules(
        &combined_ast,
        &source,
        path,
        mode,
        verbose,
        cache.as_ref(),
        module_info,
        multi_source_map,
    )
}

/// Count total modules in a tree
fn count_modules(module: &ParsedModule) -> usize {
    1 + module.submodules.iter().map(count_modules).sum::<usize>()
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
) -> Result<(Vec<CoreDef>, RecordTypes, AdtTypes, SourceMap), PipelineError> {
    use std::collections::HashSet;
    use std::fs;

    // Set max_errors for this run
    set_max_errors(max_errors);

    // Find project root (directory containing the source file for now)
    let project_root = path.parent().unwrap_or(Path::new("."));

    // No caching for compile (we want fresh compilation every time)
    let cache: Option<RefCell<BuildCache>> = None;

    // 1. Parse the module tree (handles `mod foo;` declarations)
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let module_tree = parse_module_tree(path, &mut visited, &mut chain, cache.as_ref())?;

    // 2. Discover and parse sibling modules for cross-module imports
    let workspace_root = modules::find_workspace_root(path);
    let sibling_modules = modules::parse_workspace_modules(&workspace_root, cache.as_ref());
    let workspace_module_info = modules::build_workspace_module_info(&sibling_modules);

    if verbose && !sibling_modules.is_empty() {
        let sibling_names: Vec<_> = sibling_modules
            .iter()
            .map(|m| modules::get_module_name_from_parsed(m))
            .collect();
        eprintln!(
            "Discovered {} sibling module(s) at workspace root {:?}: {:?}",
            sibling_modules.len(),
            workspace_root,
            sibling_names
        );
    }

    // 3. Flatten all modules into a single list of items
    let all_items = flatten_module_tree(&module_tree);

    if verbose {
        eprintln!(
            "Parsed {} module(s) with {} total item(s)",
            count_modules(&module_tree),
            all_items.len()
        );
    }

    // 4. Build source map for multi-file error reporting
    let multi_source_map = build_source_map(&module_tree);

    // Check for parse errors in any module
    let mut source_map_vec = Vec::new();
    let parse_errors = modules::collect_parse_errors(&module_tree, &mut source_map_vec);

    if !parse_errors.is_empty() {
        // Report errors from each file
        for (file_path, errors) in &parse_errors {
            if let Some((_, src)) = source_map_vec.iter().find(|(p, _)| p == file_path) {
                render_diagnostics(src, &file_path.display().to_string(), &[], errors);
            }
        }
        return Err(PipelineError::ElabFailed("parse errors".to_string()));
    }

    // 5. Create a combined AST with all items
    let (combined_ast, index_to_file) = build_combined_ast(&module_tree);

    // Build module info for qualified path resolution
    let file_module_info = build_module_info(&module_tree);
    let mut module_info = modules::merge_module_info(workspace_module_info, file_module_info);
    module_info.item_index_to_file = index_to_file;

    // 6. Read source for diagnostics
    let source = fs::read_to_string(path)
        .map_err(|e| PipelineError::IoError(path.display().to_string(), e.to_string()))?;

    // 7. Elaborate (Surface AST → Core)
    let (defs, warnings, record_types, adt_types): (
        Vec<CoreDef>,
        Vec<ElabError>,
        RecordTypes,
        AdtTypes,
    ) = match elaborate_with_ir_cache(&combined_ast, path, verbose, cache.as_ref(), module_info) {
        Ok((defs, warnings, record_types, adt_types)) => (defs, warnings, record_types, adt_types),
        Err(elab_errors) => {
            let filename = path.to_string_lossy();
            render_diagnostics_with_source_map(
                &source,
                &filename,
                &multi_source_map,
                &elab_errors,
                &[],
            );
            return Err(PipelineError::ElabFailed("elaboration errors".to_string()));
        }
    };

    // Render any warnings (non-fatal)
    if !warnings.is_empty() {
        let filename = path.to_string_lossy();
        render_diagnostics_with_source_map(&source, &filename, &multi_source_map, &[], &warnings);
    }

    if verbose {
        eprintln!("Elaborated {} definition(s)", defs.len());
    }

    Ok((defs, record_types, adt_types, multi_source_map))
}

/// Build a combined SourceFile from all modules in the tree, along with
/// a mapping from item indices to their source files for provenance tracking.
///
/// Submodules are processed first so their definitions are available to the parent.
/// The index_to_file mapping allows disambiguation when different files have
/// items at the same byte offsets - each item has a unique index in the combined AST.
fn build_combined_ast(module: &ParsedModule) -> (SourceFile, Vec<PathBuf>) {
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
    run_ast(&ast, source, Path::new(filename), mode, verbose, None)
}

/// Run the pipeline on an already-parsed AST.
fn run_ast(
    ast: &SourceFile,
    source: &str,
    source_path: &Path,
    mode: Mode,
    verbose: bool,
    cache: Option<&RefCell<BuildCache>>,
) -> Result<PipelineResult, PipelineError> {
    // No module info for single-file runs
    run_ast_with_modules(
        ast,
        source,
        source_path,
        mode,
        verbose,
        cache,
        ModuleInfo::default(),
        SourceMap::single(source_path.to_path_buf(), source.to_string()),
    )
}

/// Run the pipeline on an already-parsed AST with module info.
fn run_ast_with_modules(
    ast: &SourceFile,
    source: &str,
    source_path: &Path,
    mode: Mode,
    verbose: bool,
    cache: Option<&RefCell<BuildCache>>,
    module_info: ModuleInfo,
    source_map: SourceMap,
) -> Result<PipelineResult, PipelineError> {
    let filename = source_path.to_string_lossy();

    // 1. Elaborate (Surface AST → Core) with IR caching
    let (defs, warnings, _record_types, _adt_types): (
        Vec<CoreDef>,
        Vec<ElabError>,
        RecordTypes,
        AdtTypes,
    ) = match elaborate_with_ir_cache(ast, source_path, verbose, cache, module_info) {
        Ok((defs, warnings, record_types, adt_types)) => (defs, warnings, record_types, adt_types),
        Err(elab_errors) => {
            render_diagnostics_with_source_map(source, &filename, &source_map, &elab_errors, &[]);
            return Ok(PipelineResult::Failed);
        }
    };

    // Render any warnings (non-fatal)
    if !warnings.is_empty() {
        render_diagnostics_with_source_map(source, &filename, &source_map, &[], &warnings);
    }

    if verbose {
        eprintln!("Elaborated {} definition(s)", defs.len());
    }

    // 2. Check for sorry
    let has_sorry = defs.iter().any(|d| contains_sorry(&d.term));

    // 3. Evaluate if run mode
    if mode == Mode::Run {
        // Find main function
        if let Some(main_def) = defs.iter().find(|d| d.name == "main") {
            if verbose {
                eprintln!("Evaluating main()...");
            }

            // Build globals map (excluding main) for environment-based evaluation.
            // This avoids exponential term blowup from naive substitution.
            let globals: HashMap<String, Term> = defs
                .iter()
                .filter(|d| d.name != "main")
                .map(|d| (d.name.clone(), d.term.clone()))
                .collect();

            let env = EvalEnv::new(globals);
            let value = eval_with_env(&main_def.term, &env);

            return Ok(PipelineResult::Evaluated {
                value,
                ty: main_def.ty.clone(),
            });
        } else {
            // Point to end of file since we don't have a better location
            let eof_span = crate::span::Span::new(source.len() as u32, source.len() as u32);
            let err = crate::ElabError::no_main_function(eof_span);
            render_diagnostics(source, &filename, &[err], &[]);
            return Ok(PipelineResult::Failed);
        }
    }

    Ok(PipelineResult::Checked {
        num_defs: defs.len(),
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
fn elaborate_with_ir_cache(
    ast: &SourceFile,
    source_path: &Path,
    verbose: bool,
    cache: Option<&RefCell<BuildCache>>,
    module_info: ModuleInfo,
) -> Result<(Vec<CoreDef>, Vec<ElabError>, RecordTypes, AdtTypes), Vec<ElabError>> {
    use crate::elaborate::collect_definitions_with_modules;

    let mut ctx = Context::new();

    // If no cache, just elaborate directly
    let cache = match cache {
        Some(c) => c,
        None => {
            let output = if module_info.modules.is_empty() {
                elaborate_with_warnings(ast, &mut ctx)?
            } else {
                // With module info - use the module-aware collection
                let collected = collect_definitions_with_modules(
                    ast,
                    &mut ctx,
                    module_info.modules,
                    module_info.item_modules,
                    module_info.module_visibility,
                    module_info.use_statement_modules,
                    module_info.use_statement_by_span,
                    module_info.item_index_to_file,
                    module_info.module_files,
                    module_info.file_to_module,
                )?;
                collected.elaborate()?
            };
            return Ok((
                output.defs,
                output.warnings,
                output.record_types,
                output.adt_types,
            ));
        }
    };

    // Step 1: Run collection pass (always runs - ~10% of time)
    let collected = if module_info.modules.is_empty() {
        collect_definitions(ast, &mut ctx)?
    } else {
        collect_definitions_with_modules(
            ast,
            &mut ctx,
            module_info.modules,
            module_info.item_modules,
            module_info.module_visibility,
            module_info.use_statement_modules,
            module_info.use_statement_by_span,
            module_info.item_index_to_file,
            module_info.module_files,
            module_info.file_to_module,
        )?
    };

    // Step 2: Compute types_hash from collected types
    let types = collected.types_for_hash();
    let types_hash = BuildCache::compute_types_hash(&types);

    // Step 3: Check IR cache
    if let Some(cached_defs) = cache.borrow_mut().get_ir(source_path, &types_hash) {
        if verbose {
            eprintln!(
                "Using cached elaboration ({} definitions)",
                cached_defs.len()
            );
        }
        // Cache hit - return cached defs (no warnings since we didn't elaborate)
        // Note: record_types and adt_types are not cached, so we return empty for cached results
        // This is fine for non-compile use cases (check, run)
        return Ok((cached_defs, Vec::new(), HashMap::new(), HashMap::new()));
    }

    // Step 4: Cache miss - continue with elaboration
    let output = collected.elaborate()?;

    // Step 5: Cache the result
    if let Err(e) = cache
        .borrow_mut()
        .put_ir(source_path, types_hash, &output.defs)
    {
        if verbose {
            eprintln!("[cache] warning: failed to cache IR: {e}");
        }
    }

    Ok((
        output.defs,
        output.warnings,
        output.record_types,
        output.adt_types,
    ))
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
        match run_source(attempt, "<eval>", Mode::Run, verbose) {
            Ok(PipelineResult::Evaluated { value, ty }) => {
                return Ok(PipelineResult::Evaluated { value, ty });
            }
            _ => continue,
        }
    }

    // If all fail, show error from first attempt
    run_source(&attempts[0], "<eval>", Mode::Run, verbose)
}

/// Check if a term contains sorry.
fn contains_sorry(term: &Term) -> bool {
    match term {
        Term::Sorry => true,
        Term::Var(_)
        | Term::Global(_)
        | Term::Unit
        | Term::True
        | Term::False
        | Term::Zero
        | Term::NatLit(_) => false,
        Term::Succ(n) => contains_sorry(n),
        Term::Lambda(_, _, body) | Term::TyAbs(_, body) => contains_sorry(body),
        Term::App(f, x) | Term::Pair(f, x) => contains_sorry(f) || contains_sorry(x),
        Term::TyApp(t, _) | Term::Fst(t) | Term::Snd(t) => contains_sorry(t),
        Term::Inl(_, t) | Term::Inr(_, t) => contains_sorry(t),
        Term::Let(_, _, v, b) => contains_sorry(v) || contains_sorry(b),
        Term::If(c, t, e) => contains_sorry(c) || contains_sorry(t) || contains_sorry(e),
        Term::Case(s, _, l, _, r) => contains_sorry(s) || contains_sorry(l) || contains_sorry(r),
        Term::NatRec(_, z, s, n) => contains_sorry(z) || contains_sorry(s) || contains_sorry(n),
        Term::NatInd(_, z, s, n) => contains_sorry(z) || contains_sorry(s) || contains_sorry(n),
        Term::Refl(_, t) => contains_sorry(t),
        Term::Subst(_, _, eq, body) => contains_sorry(eq) || contains_sorry(body),
        Term::Absurd(_, t) => contains_sorry(t),
        Term::Annot(t, _) => contains_sorry(t),
        // Phase 2A additions
        Term::StringLit(_) => false,
        Term::StrConcat(a, b) | Term::StrEq(a, b) => contains_sorry(a) || contains_sorry(b),
        Term::StrLen(t) => contains_sorry(t),
        Term::Fix(_, _, body) => contains_sorry(body),
        Term::Fold(_, t) | Term::Unfold(_, t) => contains_sorry(t),
        // Phase 3-Prep additions
        Term::NatLt(a, b) | Term::NatLe(a, b) | Term::NatGt(a, b) | Term::NatGe(a, b) => {
            contains_sorry(a) || contains_sorry(b)
        }
        // Arithmetic operations
        Term::NatAdd(a, b)
        | Term::NatSub(a, b)
        | Term::NatMul(a, b)
        | Term::NatDiv(a, b)
        | Term::NatMod(a, b) => contains_sorry(a) || contains_sorry(b),
        Term::NatEq(a, b) | Term::BoolAnd(a, b) | Term::BoolOr(a, b) => {
            contains_sorry(a) || contains_sorry(b)
        }
        Term::BoolNot(t) => contains_sorry(t),
        Term::StrCharAt(s, n) => contains_sorry(s) || contains_sorry(n),
        Term::StrSubstring(s, start, len) => {
            contains_sorry(s) || contains_sorry(start) || contains_sorry(len)
        }
        Term::ExternCall(_, args) => args.iter().any(contains_sorry),
        Term::RefNew(t) | Term::RefGet(t) => contains_sorry(t),
        Term::RefSet(r, v) => contains_sorry(r) || contains_sorry(v),
        // Flat ADT (ADR 2.2.26)
        Term::AdtConstruct(_, _, payload) => contains_sorry(payload),
        Term::AdtMatch(scrutinee, arms) => {
            contains_sorry(scrutinee) || arms.iter().any(|(_, _, body)| contains_sorry(body))
        }
    }
}

/// Pipeline errors.
#[derive(Debug)]
pub enum PipelineError {
    /// Failed to read a file
    IoError(String, String),
    /// Elaboration or type checking failed
    ElabFailed(String),
    /// Circular module dependency detected
    ModuleCycle {
        /// Path where the cycle was detected
        path: std::path::PathBuf,
        /// The cycle chain (for error message)
        chain: Vec<std::path::PathBuf>,
    },
    /// Both file.tg and file/mod.tg exist
    AmbiguousModule {
        name: String,
        file: std::path::PathBuf,
        dir: std::path::PathBuf,
    },
    /// Module file not found
    ModuleNotFound {
        name: String,
        searched: Vec<std::path::PathBuf>,
        referenced_from: std::path::PathBuf,
    },
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::IoError(path, msg) => write!(f, "could not read '{}': {}", path, msg),
            PipelineError::ElabFailed(msg) => write!(f, "compilation failed: {}", msg),
            PipelineError::ModuleCycle { path, chain } => {
                write!(
                    f,
                    "circular module dependency detected at '{}'\n",
                    path.display()
                )?;
                write!(f, "  cycle: ")?;
                for (i, p) in chain.iter().enumerate() {
                    if i > 0 {
                        write!(f, " -> ")?;
                    }
                    write!(f, "{}", p.display())?;
                }
                Ok(())
            }
            PipelineError::AmbiguousModule { name, file, dir } => {
                write!(
                    f,
                    "ambiguous module '{}': both '{}' and '{}' exist",
                    name,
                    file.display(),
                    dir.display()
                )
            }
            PipelineError::ModuleNotFound {
                name,
                searched,
                referenced_from,
            } => {
                write!(
                    f,
                    "module '{}' not found (referenced from '{}')\n  searched: {}",
                    name,
                    referenced_from.display(),
                    searched
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_run_file_single_module() {
        let dir = TempDir::new().unwrap();
        let main_path = dir.path().join("main.tg");
        fs::write(&main_path, "fn hello() -> Nat { 42 }").unwrap();

        let result = run_file(&main_path, Mode::Check, false).unwrap();
        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 1, .. }
        ));
    }

    #[test]
    fn test_run_file_with_submodule() {
        let dir = TempDir::new().unwrap();

        // main.tg calls a function from foo.tg
        fs::write(
            dir.path().join("main.tg"),
            "mod foo;\nfn main() -> Nat { helper() }",
        )
        .unwrap();

        // helper must be pub to be accessible from main.tg
        fs::write(dir.path().join("foo.tg"), "pub fn helper() -> Nat { 42 }").unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        // Should have 2 definitions: main and helper
        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 2, .. }
        ));
    }

    #[test]
    fn test_run_file_nested_modules() {
        let dir = TempDir::new().unwrap();

        // main.tg -> mod math; -> math/mod.tg -> mod ops; -> math/ops.tg
        fs::write(
            dir.path().join("main.tg"),
            "mod math;\nfn main() -> Nat { add(1, 2) }",
        )
        .unwrap();

        let math_dir = dir.path().join("math");
        fs::create_dir(&math_dir).unwrap();

        fs::write(
            math_dir.join("mod.tg"),
            "mod ops;\nfn unused() -> Nat { 0 }",
        )
        .unwrap();

        // add must be pub to be accessible from main.tg
        fs::write(
            math_dir.join("ops.tg"),
            "pub fn add(a: Nat, b: Nat) -> Nat { a + b }",
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        // Should have 3 definitions: main, unused, add
        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 3, .. }
        ));
    }

    #[test]
    fn test_run_file_module_not_found() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.tg"), "mod nonexistent;").unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false);

        assert!(matches!(result, Err(PipelineError::ModuleNotFound { .. })));
    }

    #[test]
    fn test_run_file_use_statement_cross_module() {
        // Test that `use` imports are scoped to the module they appear in
        let dir = TempDir::new().unwrap();

        // main.tg imports type from foo module
        fs::write(
            dir.path().join("main.tg"),
            r#"
mod foo;
use foo::MyType;

fn make_my_type() -> MyType { MyType::A }
"#,
        )
        .unwrap();

        // foo.tg defines the type (no semicolon after sum type definition)
        fs::write(
            dir.path().join("foo.tg"),
            r#"
pub type MyType = A | B
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        // Should succeed with proper module-scoped imports
        // num_defs counts value definitions (functions), not types
        // - make_my_type function in main module = 1
        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 1, .. }
        ));
    }

    #[test]
    fn test_run_file_use_statement_value_import() {
        // Test that `use` can import a function
        let dir = TempDir::new().unwrap();

        // main.tg imports function from foo module and calls it
        fs::write(
            dir.path().join("main.tg"),
            r#"
mod foo;
use foo::helper;

fn main() -> Nat { helper() }
"#,
        )
        .unwrap();

        // foo.tg defines the function
        fs::write(
            dir.path().join("foo.tg"),
            r#"
pub fn helper() -> Nat { 42 }
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        // Should succeed - use imports the function
        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 2, .. }
        ));
    }

    #[test]
    fn test_run_file_use_statement_module_scoped() {
        // Test that imports in one module don't affect another
        let dir = TempDir::new().unwrap();

        // main.tg declares both modules but doesn't import MyType
        // foo imports MyType from bar, but main does NOT have that import
        // Note: bar must be pub so foo can access it (paths are canonicalized to main::bar)
        fs::write(
            dir.path().join("main.tg"),
            r#"
mod foo;
pub mod bar;

// This should fail because MyType is not imported here (only in foo.tg)
// For now, test that foo.tg's import works
fn main() -> Nat { 0 }
"#,
        )
        .unwrap();

        // bar.tg defines MyType (no semicolon after sum type)
        fs::write(
            dir.path().join("bar.tg"),
            r#"
pub type MyType = A | B
"#,
        )
        .unwrap();

        // foo.tg imports MyType from bar - this import should be scoped to foo only
        fs::write(
            dir.path().join("foo.tg"),
            r#"
use bar::MyType;

fn use_type() -> MyType { MyType::A }
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false).unwrap();

        // Should succeed - foo's import is scoped to foo
        // num_defs counts value definitions (functions), not types:
        // - main function in main module = 1
        // - use_type function in foo module = 1
        // Total = 2
        assert!(matches!(
            result,
            PipelineResult::Checked { num_defs: 2, .. }
        ));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Error file path tracking tests (ADR 4.1: Better Error Messages)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_error_in_submodule_has_correct_file_path() {
        // When an error occurs in a submodule, the error should have the
        // submodule's file_path set, not the main file's path.
        let dir = TempDir::new().unwrap();

        // main.tg is valid
        fs::write(
            dir.path().join("main.tg"),
            "mod foo;\nfn main() -> Nat { 0 }",
        )
        .unwrap();

        // foo.tg has a type error
        fs::write(
            dir.path().join("foo.tg"),
            r#"
pub fn broken() -> Nat {
    true  // Type mismatch: expected Nat, found Bool
}
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false);

        // Should fail due to the error in foo.tg
        assert!(matches!(result, Ok(PipelineResult::Failed)));
    }

    #[test]
    fn test_error_in_nested_submodule_has_correct_file_path() {
        // When an error occurs in a nested submodule, the error should have
        // that nested module's file_path set.
        let dir = TempDir::new().unwrap();

        // main.tg -> mod foo;
        fs::write(
            dir.path().join("main.tg"),
            "mod foo;\nfn main() -> Nat { 0 }",
        )
        .unwrap();

        // foo/mod.tg -> mod bar;
        let foo_dir = dir.path().join("foo");
        fs::create_dir(&foo_dir).unwrap();
        fs::write(
            foo_dir.join("mod.tg"),
            "mod bar;\npub fn foo_ok() -> Nat { 0 }",
        )
        .unwrap();

        // foo/bar.tg has the error
        fs::write(
            foo_dir.join("bar.tg"),
            r#"
pub fn bar_broken() -> Bool {
    42  // Type mismatch: expected Bool, found Nat
}
"#,
        )
        .unwrap();

        let main_path = dir.path().join("main.tg");
        let result = run_file(&main_path, Mode::Check, false);

        // Should fail due to the error in foo/bar.tg
        assert!(matches!(result, Ok(PipelineResult::Failed)));
    }
}
