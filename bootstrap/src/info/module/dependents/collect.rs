//! Collection and classification for `info module dependents` (ADR 5.9.26f).
//!
//! Everything here is pure over a parsed tree and its `ModuleInfo`: the walk
//! that records what each `use` declaration names, the fixpoint that finds the
//! re-export hops, the direct/indirect split (D2) and the string-literal scan
//! (D3). No I/O, so the split is assertable against an injected fixture.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use tungsten_bootstrap::ast::{ExpandedUseTree, Item, Path as AstPath, UseDecl, Visibility};
use tungsten_bootstrap::driver::{
    get_module_name_from_parsed, resolve_pub_use_module, ModuleInfo, ParsedModule,
};
use tungsten_bootstrap::elaborate::ModulePath;

use super::{Classified, Dependent, UseSite};

/// Walk the tree and record every module reference every `use` declaration makes.
pub fn collect_use_sites(
    module: &ParsedModule,
    current: &ModulePath,
    sources: &HashMap<PathBuf, String>,
    out: &mut Vec<UseSite>,
) {
    for item in &module.source_file.items {
        let Item::Use(decl) = item else { continue };
        let is_pub = matches!(decl.visibility, Visibility::Public | Visibility::Crate);
        let line = sources
            .get(&module.path)
            .map_or(0, |s| line_of(s, decl.span.start));
        for (prefix, name) in use_refs(decl) {
            out.push(UseSite {
                module: current.clone(),
                file: module.path.clone(),
                line,
                text: ref_text(is_pub, &prefix, name.as_deref()),
                prefix,
                item: name,
                is_pub,
            });
        }
    }
    for sub in &module.submodules {
        let child = current.child(get_module_name_from_parsed(sub));
        collect_use_sites(sub, &child, sources, out);
    }
}

/// The (module prefix, item) pairs one `use` declaration names.
fn use_refs(decl: &UseDecl) -> Vec<(Vec<String>, Option<String>)> {
    let mut refs = Vec::new();
    for expanded in decl.tree.expand_all() {
        match expanded {
            ExpandedUseTree::Paths(paths) => {
                refs.extend(paths.iter().filter_map(split_path));
            }
            ExpandedUseTree::Glob { prefix, .. } => {
                let segments = prefix.segments.iter().map(|s| s.name.clone()).collect();
                refs.push((segments, None));
            }
            ExpandedUseTree::Alias { path, .. } => {
                refs.extend(split_path(&path));
            }
        }
    }
    refs
}

/// Split `a::b::Item` into its module prefix and item name.
///
/// `None` for a single-segment path: it names no module, so there is no prefix
/// to classify against the target. That boundary decides whether a `use`
/// reference enters the census at all, which is why it is reachable from a
/// test rather than folded into [`use_refs`].
#[must_use]
pub fn split_path(path: &AstPath) -> Option<(Vec<String>, Option<String>)> {
    let n = path.segments.len();
    if n < 2 {
        return None;
    }
    let prefix = path.segments[..n - 1]
        .iter()
        .map(|s| s.name.clone())
        .collect();
    Some((prefix, Some(path.segments[n - 1].name.clone())))
}

/// Render one reference the way the report shows it.
fn ref_text(is_pub: bool, prefix: &[String], item: Option<&str>) -> String {
    let vis = if is_pub { "pub " } else { "" };
    format!("{vis}use {}::{}", prefix.join("::"), item.unwrap_or("*"))
}

/// 1-based line holding `offset`.
#[must_use]
#[allow(clippy::naive_bytecount)] // Reason: one count per use declaration, not a hot path
pub fn line_of(src: &str, offset: u32) -> usize {
    let end = (offset as usize).min(src.len());
    1 + src.as_bytes()[..end]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

/// Modules that re-export the target's items, transitively.
///
/// A `pub use` whose source is already in the set makes its own module a hop,
/// so the fixpoint walks the whole chain: `types` → `ffi` → `driver`.
#[must_use]
pub fn reexporters_of(
    sites: &[UseSite],
    info: &ModuleInfo,
    target: &ModulePath,
) -> HashSet<ModulePath> {
    let mut set = HashSet::new();
    set.insert(target.clone());
    loop {
        let mut changed = false;
        for site in sites.iter().filter(|s| s.is_pub) {
            if set.contains(&site.module) {
                continue;
            }
            let Some(src) = resolve_pub_use_module(&site.prefix, &site.module, info) else {
                continue;
            };
            if set.contains(&src) {
                set.insert(site.module.clone());
                changed = true;
            }
        }
        if !changed {
            return set;
        }
    }
}

/// Split every use site against the target module (D2).
#[must_use]
pub fn classify(sites: &[UseSite], info: &ModuleInfo, target: &ModulePath) -> Classified {
    let hops = reexporters_of(sites, info, target);
    let own_items = items_of(info, target);
    let mut out = Classified::default();

    for site in sites {
        let Some(src) = resolve_pub_use_module(&site.prefix, &site.module, info) else {
            out.unresolved += 1;
            continue;
        };
        out.resolved += 1;
        if site.module == *target {
            continue; // a module is not its own dependent
        }
        if src.starts_with(target) {
            out.direct.push(dependent(site, None));
        } else if hops.contains(&src) && reaches_target(site, &own_items) {
            out.indirect.push(dependent(site, Some(src)));
        }
    }
    out.direct.sort_by_key(order_key);
    out.indirect.sort_by_key(order_key);
    out
}

/// Whether a site importing through a re-exporter actually takes a target item.
fn reaches_target(site: &UseSite, own_items: &HashSet<String>) -> bool {
    site.item
        .as_ref()
        .is_none_or(|name| own_items.contains(name))
}

/// Every type, value and constructor the target module exports.
fn items_of(info: &ModuleInfo, target: &ModulePath) -> HashSet<String> {
    info.modules.get(target).map_or_else(HashSet::new, |c| {
        c.types
            .iter()
            .chain(c.values.iter())
            .chain(c.constructors.iter())
            .cloned()
            .collect()
    })
}

fn dependent(site: &UseSite, via: Option<ModulePath>) -> Dependent {
    Dependent {
        file: site.file.clone(),
        line: site.line,
        text: site.text.clone(),
        via,
    }
}

fn order_key(d: &Dependent) -> (String, usize, String) {
    (d.file.display().to_string(), d.line, d.text.clone())
}

/// String literals on `src`'s lines whose text contains `needle` (D3).
///
/// A text match by construction: no resolved table holds a module path spelled
/// into a literal, so this is the honest instrument and the report says so.
#[must_use]
pub fn find_literal_refs(src: &str, needle: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        for lit in string_literals_in(line) {
            if lit.contains(needle) {
                out.push((idx + 1, lit));
            }
        }
    }
    out
}

/// The contents of each double-quoted literal on one line.
fn string_literals_in(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut inside = false;
    let mut escaped = false;
    for ch in line.chars() {
        if !inside {
            inside = ch == '"';
        } else if escaped {
            buf.push(ch);
            escaped = false;
        } else if ch == '\\' {
            buf.push(ch);
            escaped = true;
        } else if ch == '"' {
            out.push(std::mem::take(&mut buf));
            inside = false;
        } else {
            buf.push(ch);
        }
    }
    out
}
