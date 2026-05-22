//! `tungsten diff-core` — structural Core IR comparison.
//!
//! Compares two Core IR dump files (from `--dump-ir=*`) structurally,
//! highlighting definition-level and type/term-level differences.
//!
//! Phase 3A: basic parallel walk with first-divergence detection.
//! See ADR 18.4.26h §5 for design rationale.

pub(crate) mod parser;
pub(crate) mod sexpr;

#[cfg(test)]
mod tests_parser;
#[cfg(test)]
mod tests_sexpr;
#[cfg(test)]
mod tests_structural_diff;

use std::path::Path;
use std::process::ExitCode;

use parser::{parse_core_defs, CoreDefs};
use sexpr::Divergence;

/// Maximum structural divergences to report per field.
const MAX_DIVERGENCES: usize = 10;

/// A single difference between two Core IR dumps.
#[derive(Debug)]
struct CoreDiff {
    kind: DiffKind,
    name: String,
    detail: String,
    type_divergences: Vec<Divergence>,
    term_divergences: Vec<Divergence>,
}

#[derive(Debug)]
enum DiffKind {
    Added,
    Removed,
    TypeChanged,
    TermChanged,
    BothChanged,
}

/// Entry point for `tungsten diff-core`.
pub fn cmd_diff_core(file_a: &Path, file_b: &Path, json: bool) -> ExitCode {
    // Validate inputs
    if !file_a.exists() {
        eprintln!("error: file not found: {}", file_a.display());
        return ExitCode::FAILURE;
    }
    if !file_b.exists() {
        eprintln!("error: file not found: {}", file_b.display());
        return ExitCode::FAILURE;
    }

    let text_a = match std::fs::read_to_string(file_a) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: reading {}: {}", file_a.display(), e);
            return ExitCode::FAILURE;
        }
    };
    let text_b = match std::fs::read_to_string(file_b) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: reading {}: {}", file_b.display(), e);
            return ExitCode::FAILURE;
        }
    };

    let defs_a = parse_core_defs(&text_a);
    let defs_b = parse_core_defs(&text_b);

    let diffs = diff_core_defs(&defs_a, &defs_b);

    if json {
        print_json_diffs(&diffs);
    } else {
        print_text_diffs(&diffs, file_a, file_b);
    }

    if diffs.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Compare two sets of parsed Core IR definitions.
fn diff_core_defs(a: &CoreDefs, b: &CoreDefs) -> Vec<CoreDiff> {
    let mut diffs = Vec::new();

    // Definitions in A but not in B (removed)
    let mut a_names: Vec<&String> = a.defs.keys().collect();
    a_names.sort();
    for name in &a_names {
        if !b.defs.contains_key(*name) {
            diffs.push(CoreDiff {
                kind: DiffKind::Removed,
                name: (*name).clone(),
                detail: "definition removed".to_string(),
                type_divergences: vec![],
                term_divergences: vec![],
            });
        }
    }

    // Definitions in B but not in A (added)
    let mut b_names: Vec<&String> = b.defs.keys().collect();
    b_names.sort();
    for name in &b_names {
        if !a.defs.contains_key(*name) {
            diffs.push(CoreDiff {
                kind: DiffKind::Added,
                name: (*name).clone(),
                detail: "definition added".to_string(),
                type_divergences: vec![],
                term_divergences: vec![],
            });
        }
    }

    // Definitions in both — compare types and terms with structural diff
    for name in &a_names {
        if let (Some(def_a), Some(def_b)) = (a.defs.get(*name), b.defs.get(*name)) {
            let type_changed = def_a.ty != def_b.ty;
            let term_changed = def_a.term != def_b.term;

            if !type_changed && !term_changed {
                continue;
            }

            let type_divergences = if type_changed {
                sexpr::structural_diff(
                    &sexpr::parse_sexpr(&def_a.ty),
                    &sexpr::parse_sexpr(&def_b.ty),
                    MAX_DIVERGENCES,
                )
            } else {
                vec![]
            };

            let term_divergences = if term_changed {
                sexpr::structural_diff(
                    &sexpr::parse_sexpr(&def_a.term),
                    &sexpr::parse_sexpr(&def_b.term),
                    MAX_DIVERGENCES,
                )
            } else {
                vec![]
            };

            let (kind, detail) = match (type_changed, term_changed) {
                (true, true) => (
                    DiffKind::BothChanged,
                    format_both_diff(&def_a.ty, &def_b.ty, &def_a.term, &def_b.term),
                ),
                (true, false) => (
                    DiffKind::TypeChanged,
                    format!("type changed:\n  a: {}\n  b: {}", def_a.ty, def_b.ty),
                ),
                (false, true) => (
                    DiffKind::TermChanged,
                    format!(
                        "term changed:\n  a: {}\n  b: {}",
                        truncate(&def_a.term, 200),
                        truncate(&def_b.term, 200)
                    ),
                ),
                _ => unreachable!(),
            };

            diffs.push(CoreDiff {
                kind,
                name: (*name).clone(),
                detail,
                type_divergences,
                term_divergences,
            });
        }
    }

    diffs
}

fn format_both_diff(ty_a: &str, ty_b: &str, term_a: &str, term_b: &str) -> String {
    format!(
        "type changed:\n  a: {}\n  b: {}\nterm changed:\n  a: {}\n  b: {}",
        ty_a,
        ty_b,
        truncate(term_a, 200),
        truncate(term_b, 200)
    )
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let end = s
            .char_indices()
            .take_while(|(i, _)| *i < max_len)
            .last()
            .map_or(0, |(i, c)| i + c.len_utf8());
        &s[..end]
    }
}

fn print_text_diffs(diffs: &[CoreDiff], file_a: &Path, file_b: &Path) {
    if diffs.is_empty() {
        println!("Core IR structurally identical.");
        return;
    }

    println!("Core IR diff: {} vs {}", file_a.display(), file_b.display());
    println!("{}", "═".repeat(60));
    println!();

    for diff in diffs {
        let label = match diff.kind {
            DiffKind::Added => "ADDED",
            DiffKind::Removed => "REMOVED",
            DiffKind::TypeChanged => "TYPE CHANGED",
            DiffKind::TermChanged => "TERM CHANGED",
            DiffKind::BothChanged => "TYPE+TERM CHANGED",
        };
        println!("[{label}] {}", diff.name);
        for line in diff.detail.lines() {
            println!("  {line}");
        }
        if !diff.type_divergences.is_empty() {
            println!("  structural divergences (type):");
            for (j, div) in diff.type_divergences.iter().enumerate() {
                println!(
                    "    [{}] depth {}, path: {}",
                    j + 1,
                    div.depth,
                    sexpr::format_path(&div.path)
                );
                println!("        a: {}", div.left);
                println!("        b: {}", div.right);
            }
        }
        if !diff.term_divergences.is_empty() {
            println!("  structural divergences (term):");
            for (j, div) in diff.term_divergences.iter().enumerate() {
                println!(
                    "    [{}] depth {}, path: {}",
                    j + 1,
                    div.depth,
                    sexpr::format_path(&div.path)
                );
                println!("        a: {}", div.left);
                println!("        b: {}", div.right);
            }
        }
        println!();
    }

    println!("Total differences: {}", diffs.len());
}

fn print_json_diffs(diffs: &[CoreDiff]) {
    print!("[");
    for (i, diff) in diffs.iter().enumerate() {
        if i > 0 {
            print!(",");
        }
        let kind = match diff.kind {
            DiffKind::Added => "added",
            DiffKind::Removed => "removed",
            DiffKind::TypeChanged => "type_changed",
            DiffKind::TermChanged => "term_changed",
            DiffKind::BothChanged => "both_changed",
        };
        let type_divs = format_divergences_json(&diff.type_divergences);
        let term_divs = format_divergences_json(&diff.term_divergences);
        print!(
            "{{\"kind\":\"{}\",\"name\":\"{}\",\"detail\":\"{}\",\"type_divergences\":{},\"term_divergences\":{}}}",
            kind,
            json_escape(&diff.name),
            json_escape(&diff.detail),
            type_divs,
            term_divs
        );
    }
    println!("]");
}

fn format_divergences_json(divs: &[Divergence]) -> String {
    if divs.is_empty() {
        return "[]".to_string();
    }
    let entries: Vec<String> = divs
        .iter()
        .map(|d| {
            format!(
                "{{\"depth\":{},\"path\":\"{}\",\"left\":\"{}\",\"right\":\"{}\"}}",
                d.depth,
                json_escape(&sexpr::format_path(&d.path)),
                json_escape(&d.left),
                json_escape(&d.right)
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Minimal JSON string escaping.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
