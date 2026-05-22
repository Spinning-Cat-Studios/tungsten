//! Level-set extraction from topological sort (ADR 11.5.26b §P4).
//!
//! Sorts sibling submodules into dependency levels for potential parallel
//! elaboration. Level 0 has no dependencies, level 1 depends only on level 0, etc.

use std::collections::HashMap;

use crate::ast::{Item, UseDecl};
use crate::driver::modules;
use crate::driver::modules::ParsedModule;

/// Sort sibling submodules by dependency order (topological sort).
///
/// Extracts `use` declarations from each submodule to determine which siblings
/// it depends on. Returns indices in dependency order (depended-on modules first).
/// Falls back to declaration order if there are cycles.
pub fn sort_submodules_by_deps(submodules: &[ParsedModule]) -> Vec<usize> {
    if submodules.len() <= 1 {
        return (0..submodules.len()).collect();
    }

    let names: Vec<String> = submodules
        .iter()
        .map(|m| modules::get_module_name_from_parsed(m))
        .collect();
    let name_to_idx: HashMap<String, usize> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();

    let deps = extract_sibling_deps(submodules, &name_to_idx);
    topo_sort_kahns(&deps)
}

/// Sort submodules into dependency level sets (ADR 11.5.26b §P4).
///
/// Returns a `Vec<Vec<usize>>` where each inner `Vec` is a set of module indices
/// at the same dependency depth. Level 0 has no dependencies, level 1 depends only
/// on level 0, etc.
///
/// # Level-set independence invariant
///
/// A level set is valid for parallel elaboration **only if** no module in the
/// set imports, re-exports, or otherwise depends on another module in the same
/// set. This must hold after accounting for: import resolution, implicit
/// prelude imports, glob expansion, and re-exports. Callers must verify this
/// invariant before using level sets for concurrent elaboration.
///
/// Falls back to one module per level if a cycle is detected.
pub fn sort_submodules_into_levels(submodules: &[ParsedModule]) -> Vec<Vec<usize>> {
    let n = submodules.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![vec![0]];
    }

    let names: Vec<String> = submodules
        .iter()
        .map(|m| modules::get_module_name_from_parsed(m))
        .collect();
    let name_to_idx: HashMap<String, usize> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();

    let deps = extract_sibling_deps(submodules, &name_to_idx);
    topo_sort_into_levels(&deps)
}

/// Extract sibling dependency edges from `use` declarations.
fn extract_sibling_deps(
    submodules: &[ParsedModule],
    name_to_idx: &HashMap<String, usize>,
) -> Vec<Vec<usize>> {
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); submodules.len()];
    for (i, submodule) in submodules.iter().enumerate() {
        collect_use_deps_recursive(submodule, i, name_to_idx, &mut deps);
    }
    deps
}

/// Recursively collect use-statement dependencies from a module and its children.
fn collect_use_deps_recursive(
    module: &ParsedModule,
    owner_idx: usize,
    name_to_idx: &HashMap<String, usize>,
    deps: &mut [Vec<usize>],
) {
    for item in &module.source_file.items {
        if let Item::Use(use_decl) = item {
            for seg in use_first_segments(use_decl) {
                if let Some(&dep_idx) = name_to_idx.get(&seg) {
                    if dep_idx != owner_idx && !deps[owner_idx].contains(&dep_idx) {
                        deps[owner_idx].push(dep_idx);
                    }
                }
            }
        }
    }
    for child in &module.submodules {
        collect_use_deps_recursive(child, owner_idx, name_to_idx, deps);
    }
}

/// Extract the first path segment from each path in a use declaration.
pub fn use_first_segments(use_decl: &UseDecl) -> Vec<String> {
    let paths = use_decl.tree.expand();
    match &paths {
        crate::ast::ExpandedUseTree::Paths(paths) => paths
            .iter()
            .filter(|p| p.segments.len() >= 2)
            .map(|p| p.segments[0].name.clone())
            .collect(),
        crate::ast::ExpandedUseTree::Glob { prefix, .. } => {
            if !prefix.segments.is_empty() {
                vec![prefix.segments[0].name.clone()]
            } else {
                Vec::new()
            }
        }
        crate::ast::ExpandedUseTree::Alias { path, .. } => {
            if path.segments.len() >= 2 {
                vec![path.segments[0].name.clone()]
            } else {
                Vec::new()
            }
        }
    }
}

/// Topological sort via Kahn's algorithm.
pub(super) fn topo_sort_kahns(deps: &[Vec<usize>]) -> Vec<usize> {
    let n = deps.len();
    let mut in_deg: Vec<usize> = deps.iter().map(|d| d.len()).collect();

    let mut reverse: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, d) in deps.iter().enumerate() {
        for &j in d {
            reverse[j].push(i);
        }
    }

    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for (i, &deg) in in_deg.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }

    let mut sorted = Vec::with_capacity(n);
    while let Some(node) = queue.pop_front() {
        sorted.push(node);
        for &dependent in &reverse[node] {
            in_deg[dependent] -= 1;
            if in_deg[dependent] == 0 {
                queue.push_back(dependent);
            }
        }
    }

    if sorted.len() < n {
        (0..n).collect()
    } else {
        sorted
    }
}

/// Kahn's algorithm variant that returns level sets instead of a flat ordering.
pub(super) fn topo_sort_into_levels(deps: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = deps.len();
    let mut in_deg: Vec<usize> = deps.iter().map(|d| d.len()).collect();

    let mut reverse: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, d) in deps.iter().enumerate() {
        for &j in d {
            reverse[j].push(i);
        }
    }

    let mut current_level: Vec<usize> = (0..n).filter(|&i| in_deg[i] == 0).collect();

    let mut levels: Vec<Vec<usize>> = Vec::new();
    let mut total_sorted = 0;

    while !current_level.is_empty() {
        total_sorted += current_level.len();
        let mut next_level: Vec<usize> = Vec::new();

        for &node in &current_level {
            for &dependent in &reverse[node] {
                in_deg[dependent] -= 1;
                if in_deg[dependent] == 0 {
                    next_level.push(dependent);
                }
            }
        }

        levels.push(current_level);
        current_level = next_level;
    }

    if total_sorted < n {
        return (0..n).map(|i| vec![i]).collect();
    }

    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topo_sort_empty() {
        let deps: Vec<Vec<usize>> = vec![];
        assert_eq!(topo_sort_kahns(&deps), Vec::<usize>::new());
    }

    #[test]
    fn topo_sort_single() {
        let deps = vec![vec![]];
        assert_eq!(topo_sort_kahns(&deps), vec![0]);
    }

    #[test]
    fn topo_sort_no_deps() {
        let deps = vec![vec![], vec![], vec![]];
        let result = topo_sort_kahns(&deps);
        assert_eq!(result.len(), 3);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(result.contains(&2));
    }

    #[test]
    fn topo_sort_linear_chain() {
        let deps = vec![vec![1], vec![2], vec![]];
        let result = topo_sort_kahns(&deps);
        let pos = |i: usize| result.iter().position(|&x| x == i).unwrap();
        assert!(pos(2) < pos(1));
        assert!(pos(1) < pos(0));
    }

    #[test]
    fn topo_sort_diamond() {
        let deps = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let result = topo_sort_kahns(&deps);
        let pos = |i: usize| result.iter().position(|&x| x == i).unwrap();
        assert!(pos(3) < pos(1));
        assert!(pos(3) < pos(2));
        assert!(pos(1) < pos(0));
        assert!(pos(2) < pos(0));
    }

    #[test]
    fn topo_sort_cycle_falls_back() {
        let deps = vec![vec![1], vec![0]];
        assert_eq!(topo_sort_kahns(&deps), vec![0, 1]);
    }

    #[test]
    fn level_set_empty() {
        let deps: Vec<Vec<usize>> = vec![];
        let levels = topo_sort_into_levels(&deps);
        assert!(levels.is_empty());
    }

    #[test]
    fn level_set_single() {
        let deps = vec![vec![]];
        let levels = topo_sort_into_levels(&deps);
        assert_eq!(levels, vec![vec![0]]);
    }

    #[test]
    fn level_set_all_independent() {
        let deps = vec![vec![], vec![], vec![]];
        let levels = topo_sort_into_levels(&deps);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].len(), 3);
    }

    #[test]
    fn level_set_linear_chain() {
        let deps = vec![vec![1], vec![2], vec![]];
        let levels = topo_sort_into_levels(&deps);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![2]);
        assert_eq!(levels[1], vec![1]);
        assert_eq!(levels[2], vec![0]);
    }

    #[test]
    fn level_set_diamond() {
        let deps = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let levels = topo_sort_into_levels(&deps);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![3]);
        assert!(levels[1].contains(&1));
        assert!(levels[1].contains(&2));
        assert_eq!(levels[2], vec![0]);
    }

    #[test]
    fn level_set_cycle_fallback() {
        let deps = vec![vec![1], vec![0]];
        let levels = topo_sort_into_levels(&deps);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], vec![0]);
        assert_eq!(levels[1], vec![1]);
    }

    /// Helper: build a ParsedModule with the given name and use declarations.
    fn make_module(name: &str, uses: Vec<Vec<&str>>) -> ParsedModule {
        use crate::ast::{Ident, SourceFile, UseDecl, UseTree, Visibility};
        use crate::span::Span;

        let use_items: Vec<Item> = uses
            .into_iter()
            .map(|segments| {
                let path_segments: Vec<Ident> = segments
                    .iter()
                    .map(|s| Ident {
                        name: s.to_string(),
                        span: Span::new(0, 0),
                    })
                    .collect();
                Item::Use(UseDecl {
                    visibility: Visibility::Private,
                    tree: UseTree::Path(crate::ast::Path {
                        segments: path_segments,
                        span: Span::new(0, 0),
                    }),
                    span: Span::new(0, 0),
                })
            })
            .collect();

        ParsedModule {
            path: std::path::PathBuf::from(format!("{name}.tg")),
            source_file: SourceFile {
                items: use_items,
                span: Span::new(0, 0),
            },
            submodules: vec![],
            visibility: Visibility::Public,
        }
    }

    #[test]
    fn level_sets_with_real_parsed_modules() {
        // alpha has no deps, beta imports alpha, gamma imports alpha
        let alpha = make_module("alpha", vec![]);
        let beta = make_module("beta", vec![vec!["alpha", "something"]]);
        let gamma = make_module("gamma", vec![vec!["alpha", "other"]]);

        let modules = vec![alpha, beta, gamma];
        let levels = sort_submodules_into_levels(&modules);

        // alpha (index 0) at level 0, beta+gamma (indices 1,2) at level 1
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], vec![0]);
        assert_eq!(levels[1].len(), 2);
        assert!(levels[1].contains(&1));
        assert!(levels[1].contains(&2));
    }
}
