//! `tungsten info module dependents` — who depends on a module, and by which path.
//!
//! The inverse of `info module imports` (ADR 5.9.26f). A directory regroup turns
//! on one question: which references name the module *in their path*, and so
//! change when the path changes? `grep` cannot answer it —
//! `use driver::ffi::{List}` rides a re-export and survives any move, while
//! `pub use driver::ffi::types::{…}` names the submodule and does not, and the
//! two are indistinguishable to a text search. This inverts the table the
//! elaborator already built and reports the two populations separately (D2).
//!
//! String literals holding a module path are a third population no resolved
//! table can see (D3): the test harness spells `driver::ffi::process::println`
//! into source it generates. They are found by text and labelled as such.

pub mod collect;
pub mod render;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_bootstrap::driver::{
    self, build_module_info, parse_module_tree, ModuleInfo, ParsedModule,
};
use tungsten_bootstrap::elaborate::ModulePath;

pub use collect::{classify, collect_use_sites, find_literal_refs};

/// One module reference made by one `use` declaration, before classification.
#[derive(Debug, Clone)]
pub struct UseSite {
    /// The module containing the declaration.
    pub module: ModulePath,
    /// The file the declaration is written in.
    pub file: PathBuf,
    /// 1-based line of the declaration.
    pub line: usize,
    /// The module segments named before the item (`driver::ffi::types`).
    pub prefix: Vec<String>,
    /// The item named, or `None` for a glob — a glob takes whatever the prefix
    /// exports, so there is no name to match against the target's own items.
    pub item: Option<String>,
    /// Whether this is a `pub use`, and therefore itself a re-export hop.
    pub is_pub: bool,
    /// The declaration as rendered for the report.
    pub text: String,
}

/// A use site classified against the target module.
#[derive(Debug, Clone)]
pub struct Dependent {
    /// The file the declaration is written in.
    pub file: PathBuf,
    /// 1-based line of the declaration.
    pub line: usize,
    /// The declaration as rendered for the report.
    pub text: String,
    /// For an indirect site, the re-exporting module it reaches the items through.
    pub via: Option<ModulePath>,
}

/// A `.tg` string literal whose text contains the module path (D3).
#[derive(Debug, Clone)]
pub struct LiteralRef {
    /// The file the literal is written in.
    pub file: PathBuf,
    /// 1-based line of the literal.
    pub line: usize,
    /// The literal's contents.
    pub text: String,
}

/// Everything `info module dependents` reports.
#[derive(Debug, Default)]
pub struct DependentsReport {
    /// The target module, as rendered.
    pub module: String,
    /// Sites naming the module in their path — a move changes these.
    pub direct: Vec<Dependent>,
    /// Sites reaching its items through a re-export — a move does not.
    pub indirect: Vec<Dependent>,
    /// Text matches in string literals (D3).
    pub literals: Vec<LiteralRef>,
    /// How many modules the parsed tree holds.
    pub modules_in_tree: usize,
    /// How many use-declaration references were examined across the tree.
    pub sites_examined: usize,
    /// How many of those named a module the resolver could not place.
    pub sites_unresolved: usize,
}

/// The direct/indirect split plus the resolution census that produced it.
#[derive(Debug, Default)]
pub struct Classified {
    /// Sites naming the target in their path.
    pub direct: Vec<Dependent>,
    /// Sites reaching the target's items through a re-export.
    pub indirect: Vec<Dependent>,
    /// Use references whose prefix resolved to some module.
    pub resolved: usize,
    /// Use references whose prefix resolved to nothing.
    pub unresolved: usize,
}

/// Entry point for `tungsten info module dependents <module> <file>`.
pub fn cmd_info_module_dependents(
    module: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = match parse_module_tree(file, &mut visited, &mut chain, None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let info = build_module_info(&tree);

    let target = match resolve_target(&info, module) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };

    // D4: cost 3. The split is only as good as the resolution behind it, and a
    // corpus the elaborator rejects has no trustworthy resolution — so refuse
    // rather than fall back to a text answer wearing a resolved answer's face
    // (§3 Non-Goals). The useful moment is before a move, on a tree that
    // compiles.
    if let Err(e) = driver::elaborate_project(file, verbose, max_errors, None) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    let sources = read_sources(&tree);
    let report = build_report(&tree, &info, &target, &sources);
    print!("{}", render::render_report(&report, verbose));
    ExitCode::SUCCESS
}

/// Resolve the `<module>` argument against the tree, or say why it did not.
///
/// Split out of the command so the arm a mis-typed module name actually hits is
/// pure over the resolved table: the refusal and its suggestions are assertable
/// with no parse, no elaboration and no captured stderr.
///
/// # Errors
/// The rendered message — the refusal and, when the last segment matches a
/// module that does exist, the `did you mean:` block naming it.
pub fn resolve_target(info: &ModuleInfo, module: &str) -> Result<ModulePath, String> {
    let segments: Vec<String> = module.split("::").map(String::from).collect();
    let target = ModulePath::from_segments(&segments);
    if target.is_root() || !info.modules.contains_key(&target) {
        let mut message = format!("module '{module}' not found in module tree");
        if let Some(hint) = near_misses(info, &segments) {
            message.push_str(&hint);
        }
        return Err(message);
    }
    Ok(target)
}

/// Module paths whose last segment matches the one asked for, rendered as the
/// `did you mean:` block, or `None` when nothing matches.
///
/// The last segment rather than the whole path, because the commonest way to
/// get this wrong is to name the leaf a regroup moved and the parent it used
/// to sit under.
fn near_misses(info: &ModuleInfo, segments: &[String]) -> Option<String> {
    let last = segments.last()?;
    let mut hits: Vec<String> = info
        .modules
        .keys()
        .filter(|p| p.segments.last() == Some(last))
        .map(ToString::to_string)
        .collect();
    if hits.is_empty() {
        return None;
    }
    hits.sort();
    let mut hint = String::from("\ndid you mean:");
    for hit in hits.iter().take(5) {
        let _ = write!(hint, "\n  {hit}");
    }
    Some(hint)
}

/// Read every source file in the tree, keyed by path.
///
/// Line numbers and the literal scan (D3) both need the text; a file that
/// cannot be read is simply absent, which costs its line numbers and its
/// literals, never the resolved split.
fn read_sources(module: &ParsedModule) -> HashMap<PathBuf, String> {
    let mut out = HashMap::new();
    collect_sources(module, &mut out);
    out
}

fn collect_sources(module: &ParsedModule, out: &mut HashMap<PathBuf, String>) {
    if let Ok(text) = std::fs::read_to_string(&module.path) {
        out.insert(module.path.clone(), text);
    }
    for sub in &module.submodules {
        collect_sources(sub, out);
    }
}

/// Build the whole report for `target`.
#[must_use]
pub fn build_report(
    tree: &ParsedModule,
    info: &ModuleInfo,
    target: &ModulePath,
    sources: &HashMap<PathBuf, String>,
) -> DependentsReport {
    let mut sites = Vec::new();
    collect_use_sites(tree, &ModulePath::root(), sources, &mut sites);
    let classified = classify(&sites, info, target);

    let needle = target.to_string();
    let mut literals = Vec::new();
    for (file, text) in sources.iter().collect::<BTreeMap<_, _>>() {
        for (line, lit) in find_literal_refs(text, &needle) {
            literals.push(LiteralRef {
                file: (*file).clone(),
                line,
                text: lit,
            });
        }
    }

    DependentsReport {
        module: needle,
        direct: classified.direct,
        indirect: classified.indirect,
        literals,
        modules_in_tree: tree.module_count(),
        sites_examined: classified.resolved + classified.unresolved,
        sites_unresolved: classified.unresolved,
    }
}

// Tests split along the source seam (CLAUDE.md § File + Folder Complexity):
// `tests` owns the shared fixture and this file's own surface, and each sibling
// is named for the source file it exercises.
#[cfg(test)]
mod collect_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tests;
