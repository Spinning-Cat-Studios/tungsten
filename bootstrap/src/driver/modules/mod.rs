//! Module resolution and tree building.
//!
//! Resolves `mod foo;` declarations to files and builds a module tree.
//!
//! ## Workspace-Aware Resolution
//!
//! When checking a single file that's part of a larger project, we need to know
//! about sibling modules for cross-module imports. For example, when checking
//! `elab/env/mod.tg`, it may `use lexer::span::Span` - we need the `lexer` module
//! to be registered even though it's not a descendant of `elab`.
//!
//! The workspace discovery system:
//! 1. Finds the "workspace root" - the directory containing the file being checked
//! 2. Discovers all sibling modules (directories with `mod.tg` or `*.tg` files)
//! 3. Parses all sibling module trees
//! 4. Merges them into a combined `ModuleInfo` for resolution

mod info;
mod parse;
mod workspace;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{SourceFile, Visibility};

// Re-export public items
pub use info::{build_module_info, ModuleInfo};
pub use parse::{
    build_source_map, collect_parse_errors, extract_module_dependencies, flatten_module_tree,
    parse_module_tree, ModuleDependencyInfo,
};
pub use workspace::{
    build_workspace_module_info, discover_sibling_modules, find_workspace_root,
    get_module_name_from_parsed, merge_module_info, parse_workspace_modules,
};

/// Source map for multi-file error reporting.
/// Maps file paths to their source content.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    /// File path → source content
    pub(crate) sources: HashMap<PathBuf, String>,
    /// Main file path (for fallback)
    pub(crate) main_file: Option<PathBuf>,
}

impl SourceMap {
    /// Create a new empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a source map with a single main file.
    pub fn single(path: PathBuf, source: String) -> Self {
        let mut sources = HashMap::new();
        sources.insert(path.clone(), source);
        Self {
            sources,
            main_file: Some(path),
        }
    }

    /// Add a source file to the map.
    pub fn insert(&mut self, path: PathBuf, source: String) {
        if self.main_file.is_none() {
            self.main_file = Some(path.clone());
        }
        self.sources.insert(path, source);
    }

    /// Get the source for a file path.
    pub fn get(&self, path: &Path) -> Option<&str> {
        self.sources.get(path).map(|s| s.as_str())
    }

    /// Get the main file's source (for fallback).
    pub fn main_source(&self) -> Option<&str> {
        self.main_file
            .as_ref()
            .and_then(|p| self.sources.get(p).map(|s| s.as_str()))
    }

    /// Get the main file path.
    pub fn main_file(&self) -> Option<&Path> {
        self.main_file.as_deref()
    }

    /// Check if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

/// A parsed module with its submodules.
#[derive(Debug)]
pub struct ParsedModule {
    /// Path to this module's source file
    pub path: PathBuf,
    /// Visibility of this module (Public or Private)
    pub visibility: Visibility,
    /// The parsed source file
    pub source_file: SourceFile,
    /// Submodules declared in this file
    pub submodules: Vec<ParsedModule>,
}
