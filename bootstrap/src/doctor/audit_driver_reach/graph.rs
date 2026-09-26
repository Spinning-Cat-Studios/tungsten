//! Turning one entry file's parsed module tree into a module-level `use` graph.
//!
//! Parse-level throughout (cost 2). The tree gives module *existence*; the
//! `use` declarations give the edges. Keying modules by their path **below the
//! root** (`codegen::ir_types`, not `main::codegen::ir_types`) is what makes two
//! entry files' graphs comparable — and it is also how `use` paths are written
//! in this repo, which is absolute-from-root with the root name omitted.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use crate::ast::{ExpandedUseTree, Item, Path as AstPath};
use crate::driver::{get_module_name_from_parsed, parse_module_tree, ParsedModule};

/// One entry file's contribution to the shared reach graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleGraph {
    /// Every module below the root, by qualified path.
    pub modules: BTreeSet<String>,
    /// Module → the modules it imports directly. Unresolved paths are dropped.
    pub uses: BTreeMap<String, BTreeSet<String>>,
    /// What the entry file itself imports. Held apart from `uses` because the
    /// entry file is a root, not a module, and must not be classified.
    pub entry_uses: BTreeSet<String>,
}

/// Parse `entry` and extract its module-level `use` graph.
///
/// `None` when the tree cannot be parsed — the caller decides whether that is
/// fatal (the driver entry) or a named omission (a test entry).
#[must_use]
pub fn module_graph(entry: &Path) -> Option<ModuleGraph> {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(entry, &mut visited, &mut chain, None).ok()?;

    let mut modules = BTreeSet::new();
    collect_modules(&tree, "", &mut modules);

    let mut graph = ModuleGraph {
        entry_uses: resolve_all(&direct_use_paths(&tree), &modules),
        modules,
        uses: BTreeMap::new(),
    };
    collect_uses(&tree, "", &graph.modules.clone(), &mut graph.uses);
    Some(graph)
}

/// Every module below `module`, qualified relative to the entry file's root.
fn collect_modules(module: &ParsedModule, prefix: &str, out: &mut BTreeSet<String>) {
    for child in &module.submodules {
        let name = get_module_name_from_parsed(child);
        let qualified = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}::{name}")
        };
        out.insert(qualified.clone());
        collect_modules(child, &qualified, out);
    }
}

/// Record each module's outgoing `use` edges, recursively.
fn collect_uses(
    module: &ParsedModule,
    prefix: &str,
    known: &BTreeSet<String>,
    out: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for child in &module.submodules {
        let name = get_module_name_from_parsed(child);
        let qualified = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}::{name}")
        };
        let mut edges = resolve_all(&direct_use_paths(child), known);
        // A module never counts as reaching itself: `codegen/mod.tg`'s
        // `pub use codegen::ir_builder::*` is a re-export, and a self-edge would
        // make every re-exporting parent look like its own importer.
        edges.remove(&qualified);
        if !edges.is_empty() {
            out.entry(qualified.clone()).or_default().extend(edges);
        }
        collect_uses(child, &qualified, known, out);
    }
}

/// The `::`-joined segment lists of every `use` declaration in one module.
fn direct_use_paths(module: &ParsedModule) -> Vec<Vec<String>> {
    let mut paths = Vec::new();
    for item in &module.source_file.items {
        let Item::Use(decl) = item else { continue };
        for expanded in decl.tree.expand_all() {
            match expanded {
                ExpandedUseTree::Paths(each) => paths.extend(each.iter().map(segments_of)),
                ExpandedUseTree::Glob { prefix, .. }
                | ExpandedUseTree::Alias { path: prefix, .. } => {
                    paths.push(segments_of(&prefix));
                }
            }
        }
    }
    paths
}

fn segments_of(path: &AstPath) -> Vec<String> {
    path.segments.iter().map(|s| s.name.clone()).collect()
}

/// Resolve `use` paths to the modules they name, dropping what does not resolve.
fn resolve_all(paths: &[Vec<String>], known: &BTreeSet<String>) -> BTreeSet<String> {
    paths
        .iter()
        .filter_map(|segments| resolve_module(segments, known))
        .collect()
}

/// The longest prefix of `segments` that names a known module.
///
/// A `use` path ends in an item name (`use codegen::ir_types::llvm_type`), and
/// nothing at parse level distinguishes an item from a submodule — so the
/// resolution rule is longest-known-prefix rather than "drop the last segment",
/// which would turn `use codegen::ir_types` (a whole-module import) into
/// `codegen`.
#[must_use]
pub fn resolve_module(segments: &[String], known: &BTreeSet<String>) -> Option<String> {
    (1..=segments.len())
        .rev()
        .map(|end| segments[..end].join("::"))
        .find(|candidate| known.contains(candidate))
}
