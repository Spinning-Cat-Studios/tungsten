//! Module tree parsing and traversal.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::ast::{Item, Visibility};
use crate::cache::BuildCache;
use crate::driver::PipelineError;
use crate::parser::Parser;

use super::{ParsedModule, SourceMap};

/// Recursively parse a module and its submodules.
///
/// This function:
/// 1. Parses the source file at `path`
/// 2. For each `mod foo;` declaration, resolves the submodule path
/// 3. Recursively parses submodules
/// 4. Detects and reports module cycles
///
/// If `cache` is provided, it will be used to skip parsing for unchanged files.
/// The root module is always treated as Public (crate root).
#[allow(clippy::implicit_hasher)] // Reason: changing to generic hasher would add complexity for no benefit
pub fn parse_module_tree(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
    chain: &mut Vec<PathBuf>,
    cache: Option<&Mutex<BuildCache>>,
) -> Result<ParsedModule, PipelineError> {
    let mut ctx = ParseCtx {
        visited,
        chain,
        cache,
        preparsed: None,
    };
    parse_module_tree_inner(path, Visibility::Public, &mut ctx)
}

/// Bundles mutable traversal state for recursive module tree building.
///
/// Used by both serial (`parse_module_tree`) and pre-parsed
/// (`parse_module_tree_with_preparsed`) paths.
struct ParseCtx<'a> {
    visited: &'a mut HashSet<PathBuf>,
    chain: &'a mut Vec<PathBuf>,
    cache: Option<&'a Mutex<BuildCache>>,
    /// When set, prefer pre-parsed ASTs over reading from disk or cache.
    preparsed: Option<&'a PreParsedMap>,
}

/// Internal helper that tracks visibility through the module tree.
///
/// When `ctx.preparsed` is `Some`, tries the pre-parsed map first before
/// falling back to the build cache or fresh parsing.
fn parse_module_tree_inner(
    path: &Path,
    visibility: Visibility,
    ctx: &mut ParseCtx<'_>,
) -> Result<ParsedModule, PipelineError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| PipelineError::IoError(path.display().to_string(), e.to_string()))?;

    // Cycle detection
    if ctx.visited.contains(&canonical) {
        return Err(PipelineError::ModuleCycle {
            path: canonical,
            chain: ctx.chain.clone(),
        });
    }

    ctx.visited.insert(canonical.clone());
    ctx.chain.push(canonical.clone());

    // Resolve source file: pre-parsed map → build cache → fresh parse
    let source_file = if let Some(ast) = ctx.preparsed.and_then(|m| m.get(&canonical)) {
        ast.clone()
    } else {
        let source = fs::read_to_string(path)
            .map_err(|e| PipelineError::IoError(path.display().to_string(), e.to_string()))?;
        if let Some(cache_cell) = ctx.cache {
            let mut c = cache_cell.lock().unwrap();
            if let Some(cached_ast) = c.get(path, &source) {
                cached_ast
            } else {
                let (ast, _) = Parser::new(&source).parse();
                let _ = c.put(path, &source, &ast);
                ast
            }
        } else {
            let (ast, _) = Parser::new(&source).parse();
            ast
        }
    };

    // Resolve submodules
    let parent_dir = path.parent().unwrap_or(Path::new("."));
    let mut submodules = Vec::new();

    for item in &source_file.items {
        if let Item::Mod(mod_decl) = item {
            let submodule_path = resolve_module_path(parent_dir, &mod_decl.name.name, path)?;
            let submodule = parse_module_tree_inner(&submodule_path, mod_decl.visibility, ctx)?;
            submodules.push(submodule);
        }
    }

    // Pop from chain (backtrack)
    ctx.chain.pop();
    ctx.visited.remove(&canonical);

    Ok(ParsedModule {
        path: path.to_path_buf(),
        visibility,
        source_file,
        submodules,
    })
}

/// Resolve `mod foo;` to a file path.
///
/// Follows Rust's resolution rules:
/// 1. Look for `foo.tg` in the same directory
/// 2. Look for `foo/mod.tg` in a subdirectory
/// 3. Error if both exist (ambiguous)
/// 4. Error if neither exists (not found)
pub(super) fn resolve_module_path(
    parent_dir: &Path,
    name: &str,
    referenced_from: &Path,
) -> Result<PathBuf, PipelineError> {
    let file_path = parent_dir.join(format!("{}.tg", name));
    let dir_path = parent_dir.join(name).join("mod.tg");

    let file_exists = file_path.exists();
    let dir_exists = dir_path.exists();

    match (file_exists, dir_exists) {
        (true, false) => Ok(file_path),
        (false, true) => Ok(dir_path),
        (true, true) => Err(PipelineError::AmbiguousModule {
            name: name.to_string(),
            file: file_path,
            dir: dir_path,
        }),
        (false, false) => Err(PipelineError::ModuleNotFound {
            name: name.to_string(),
            searched: vec![file_path, dir_path],
            referenced_from: referenced_from.to_path_buf(),
        }),
    }
}

/// Flatten a module tree into a single list of items.
///
/// All items from the module tree are collected in depth-first order.
/// `mod` declarations are excluded (they've already been resolved).
pub fn flatten_module_tree(module: &ParsedModule) -> Vec<(&Item, &Path)> {
    let mut items = Vec::new();

    // Add items from this module (excluding mod declarations)
    for item in &module.source_file.items {
        if !matches!(item, Item::Mod(_)) {
            items.push((item, module.path.as_path()));
        }
    }

    // Recursively add items from submodules
    for submodule in &module.submodules {
        items.extend(flatten_module_tree(submodule));
    }

    items
}

/// Build a SourceMap from a module tree.
/// Reads all source files for multi-file error reporting.
pub fn build_source_map(module: &ParsedModule) -> SourceMap {
    let mut source_map = SourceMap::new();
    build_source_map_recursive(module, &mut source_map, true);
    source_map
}

fn build_source_map_recursive(module: &ParsedModule, source_map: &mut SourceMap, is_main: bool) {
    // Read source for this module
    if let Ok(source) = fs::read_to_string(&module.path) {
        if is_main {
            source_map.main_file = Some(module.path.clone());
        }
        source_map.sources.insert(module.path.clone(), source);
    }

    // Recursively add submodules
    for submodule in &module.submodules {
        build_source_map_recursive(submodule, source_map, false);
    }
}

/// Collect all parse errors from a module tree.
pub fn collect_parse_errors(
    module: &ParsedModule,
    source_map: &mut Vec<(PathBuf, String)>,
) -> Vec<(PathBuf, Vec<crate::error::ParseError>)> {
    let mut errors = Vec::new();

    // Read source for this module (for error reporting)
    if let Ok(source) = fs::read_to_string(&module.path) {
        source_map.push((module.path.clone(), source.clone()));

        // Re-parse to get errors (we could cache this)
        let (_, parse_errors) = Parser::new(&source).parse();
        if !parse_errors.is_empty() {
            errors.push((module.path.clone(), parse_errors));
        }
    }

    // Recursively collect from submodules
    for submodule in &module.submodules {
        errors.extend(collect_parse_errors(submodule, source_map));
    }

    errors
}

/// Module dependency information extracted from a ParsedModule.
#[derive(Debug)]
pub struct ModuleDependencyInfo {
    /// Path to the module's source file
    pub path: PathBuf,
    /// Content hash of the source file
    pub content_hash: [u8; 32],
    /// Paths to modules this module depends on (via `mod` declarations)
    pub dependencies: Vec<PathBuf>,
}

/// Extract dependency information from a module tree.
///
/// Returns a list of (path, content_hash, dependencies) tuples for all modules,
/// suitable for building a dependency graph.
pub fn extract_module_dependencies(module: &ParsedModule) -> Vec<ModuleDependencyInfo> {
    let mut result = Vec::new();
    extract_deps_recursive(module, &mut result);
    result
}

/// Helper function for recursive dependency extraction.
fn extract_deps_recursive(module: &ParsedModule, result: &mut Vec<ModuleDependencyInfo>) {
    use crate::cache::BuildCache;

    // Compute content hash for this module
    let source = fs::read(&module.path).unwrap_or_default();
    let content_hash = BuildCache::compute_hash(&source);

    // Get dependencies (paths of submodules)
    let dependencies: Vec<PathBuf> = module.submodules.iter().map(|s| s.path.clone()).collect();

    result.push(ModuleDependencyInfo {
        path: module.path.clone(),
        content_hash,
        dependencies,
    });

    // Recursively process submodules
    for submodule in &module.submodules {
        extract_deps_recursive(submodule, result);
    }
}

// =========================================================================
// Parallel parsing (ADR 11.5.26b §2.4, P3)
// =========================================================================

/// Pre-parsed AST map: canonical path → (source, parsed AST).
pub type PreParsedMap = HashMap<PathBuf, crate::ast::SourceFile>;

/// Discover all `.tg` files under `root` recursively.
pub fn discover_tg_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    discover_tg_files_recursive(root, &mut files);
    files
}

fn discover_tg_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("warning: cannot read directory {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories and common non-source dirs
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "target" && name != "node_modules" {
                discover_tg_files_recursive(&path, files);
            }
        } else if path.extension().map_or(false, |e| e == "tg") {
            files.push(path);
        }
    }
}

/// Parse all discovered files in parallel using rayon (ADR 11.5.26b §2.4).
///
/// Returns a map from canonical path → parsed SourceFile. Files that fail
/// to read or canonicalize are skipped with a warning (errors are caught
/// later during tree building).
pub fn parse_files_parallel(paths: &[PathBuf]) -> PreParsedMap {
    use rayon::prelude::*;

    paths
        .par_iter()
        .filter_map(|path| {
            let canonical = match path.canonicalize() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("warning: cannot canonicalize {}: {e}", path.display());
                    return None;
                }
            };
            let source = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("warning: cannot read {}: {e}", path.display());
                    return None;
                }
            };
            let (ast, _errors) = Parser::new(&source).parse();
            Some((canonical, ast))
        })
        .collect()
}

/// Parse a module tree using pre-parsed ASTs where available (ADR 11.5.26b §P3).
///
/// Falls back to serial parsing for files not in the pre-parsed map.
pub fn parse_module_tree_with_preparsed(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
    chain: &mut Vec<PathBuf>,
    cache: Option<&Mutex<BuildCache>>,
    preparsed: &PreParsedMap,
) -> Result<ParsedModule, PipelineError> {
    let mut ctx = ParseCtx {
        visited,
        chain,
        cache,
        preparsed: Some(preparsed),
    };
    parse_module_tree_inner(path, Visibility::Public, &mut ctx)
}

#[cfg(test)]
mod tests;
