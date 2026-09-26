//! Scoping definitions to one module for `tungsten test --module`
//! (ADR 12.5.26b).
//!
//! Split out of `test_runner/mod.rs` for the 400-LOC file limit. Its own seam:
//! path matching, and nothing else — a suffix match that is ambiguous must say
//! so rather than picking a winner.
//!
//! Tests: `bootstrap/src/test_runner/tests.rs` — the `scope_*` cases stayed
//! with the parent when this module split out.

use std::path::{Path, PathBuf};

use tungsten_bootstrap::elaborate::CoreDef;

/// Result of scoping definitions to a target module (ADR 12.5.26b).
#[derive(Debug)]
pub(super) enum ModuleScopeResult {
    /// Exactly one module matched; contains the defs from that module.
    Matched(Vec<CoreDef>),
    /// No module matched the target path.
    NoMatch,
    /// Multiple modules matched (ambiguous suffix); contains the matching paths.
    Ambiguous(Vec<PathBuf>),
}

/// Scope definitions to a single module by matching `target` against `module_defs` source paths.
///
/// The target is normalized (strip leading `./`, canonicalize) and compared against
/// each module entry's source file path. Matches are tried as:
/// 1. Exact path match (after normalization)
/// 2. Suffix match (target is a suffix of the module source path)
///
/// If multiple modules match via suffix, returns `Ambiguous`.
pub(super) fn scope_defs_to_module(
    module_defs: &[(Vec<String>, PathBuf, Vec<CoreDef>)],
    target: &str,
    project_root: &Path,
) -> ModuleScopeResult {
    // Normalize the target: strip leading "./" and resolve relative to project_root
    let target_path = Path::new(target);
    let normalized = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        // Strip leading "./" by canonicalizing components
        let stripped = target.strip_prefix("./").unwrap_or(target);
        project_root.join(stripped)
    };

    let mut matches: Vec<(PathBuf, Vec<CoreDef>)> = Vec::new();

    for (_mod_path, source_file, defs) in module_defs {
        // Try exact match first
        if source_file == &normalized {
            return ModuleScopeResult::Matched(defs.clone());
        }

        // Try suffix match: does the module source path end with the target?
        let stripped = target.strip_prefix("./").unwrap_or(target);
        if let Ok(suffix) = Path::new(stripped).strip_prefix(".") {
            // Already stripped
            if source_file.ends_with(suffix) {
                matches.push((source_file.clone(), defs.clone()));
            }
        } else if source_file.ends_with(stripped) {
            matches.push((source_file.clone(), defs.clone()));
        }
    }

    match matches.len() {
        0 => ModuleScopeResult::NoMatch,
        1 => ModuleScopeResult::Matched(matches.into_iter().next().unwrap().1),
        _ => ModuleScopeResult::Ambiguous(matches.into_iter().map(|(p, _)| p).collect()),
    }
}
