//! Encoding depth check (ADR 20.4.26d).
//!
//! Reports maximum encoding stack depth (SCC group size), type-term depth,
//! and type-term node count across all encoded types.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;

/// Thresholds for encoding depth checks.
#[allow(clippy::struct_field_names)] // Reason: max_ prefix is intentional for threshold clarity
pub struct DepthThresholds {
    pub max_stack: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
}

impl Default for DepthThresholds {
    fn default() -> Self {
        Self {
            max_stack: 20,
            max_depth: 50,
            max_nodes: 5000,
        }
    }
}

/// Run the encoding depth check.
pub fn cmd_check_encoding_depth(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    thresholds: &DepthThresholds,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let encoded_types = &project.encoded_types;
    let mutual_groups = &project.mutual_recursion_groups;

    if encoded_types.is_empty() {
        println!("No cached type encodings found.");
        return ExitCode::SUCCESS;
    }

    println!("Encoding all types...");
    println!();

    let results = collect_metrics(encoded_types, mutual_groups);
    print_maximums(&results);

    if verbose {
        print_verbose_table(&results);
    }

    check_thresholds(&results, thresholds)
}

/// Compute metrics for each encoded type.
fn collect_metrics(
    encoded_types: &std::collections::HashMap<String, tungsten_core::Type>,
    mutual_groups: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<TypeMetrics> {
    let mut type_names: Vec<&str> = encoded_types.keys().map(|s| s.as_str()).collect();
    type_names.sort();

    type_names
        .iter()
        .map(|name| {
            let ty = &encoded_types[*name];
            TypeMetrics {
                name: (*name).to_string(),
                group_size: mutual_groups.get(*name).map(|g| g.len()).unwrap_or(0),
                term_depth: ty.depth(),
                node_count: ty.node_count(),
            }
        })
        .collect()
}

/// Print the maximum values across all metrics.
fn print_maximums(results: &[TypeMetrics]) {
    if let Some(m) = results.iter().max_by_key(|r| r.group_size) {
        println!(
            "Max encoding stack depth: {} ({}{})",
            m.group_size,
            m.name,
            if m.group_size > 1 {
                format!(", group size {}", m.group_size)
            } else {
                String::new()
            }
        );
    }
    if let Some(m) = results.iter().max_by_key(|r| r.term_depth) {
        println!(
            "Max type-term depth: {} ({} μ-encoding)",
            m.term_depth, m.name
        );
    }
    if let Some(m) = results.iter().max_by_key(|r| r.node_count) {
        println!("Max type-term node count: {} ({})", m.node_count, m.name);
    }
    println!();
}

/// Print all types sorted by node count (verbose output).
fn print_verbose_table(results: &[TypeMetrics]) {
    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| b.node_count.cmp(&a.node_count));
    println!("All types by node count:");
    for r in &sorted {
        println!(
            "  {:>5} nodes, depth {:>3}, group {:>2}  {}",
            r.node_count, r.term_depth, r.group_size, r.name
        );
    }
    println!();
}

/// Check all metrics against thresholds; return SUCCESS or FAILURE.
fn check_thresholds(results: &[TypeMetrics], t: &DepthThresholds) -> ExitCode {
    let mut violations = 0;

    for r in results {
        if r.group_size > t.max_stack {
            eprintln!(
                "FAIL: {} encoding stack depth {} exceeds threshold {}",
                r.name, r.group_size, t.max_stack
            );
            violations += 1;
        }
        if r.term_depth > t.max_depth {
            eprintln!(
                "FAIL: {} type-term depth {} exceeds threshold {}",
                r.name, r.term_depth, t.max_depth
            );
            violations += 1;
        }
        if r.node_count > t.max_nodes {
            eprintln!(
                "FAIL: {} type-term node count {} exceeds threshold {}",
                r.name, r.node_count, t.max_nodes
            );
            violations += 1;
        }
    }

    if violations == 0 {
        println!(
            "✓ All within thresholds (stack ≤ {}, depth ≤ {}, nodes ≤ {})",
            t.max_stack, t.max_depth, t.max_nodes
        );
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("{violations} threshold violation(s) found");
        ExitCode::FAILURE
    }
}

#[derive(Debug, Clone)]
struct TypeMetrics {
    name: String,
    group_size: usize,
    term_depth: usize,
    node_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_check_encoding_depth_simple() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "pub type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_encoding_depth(&path, false, 20, &DepthThresholds::default());
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_check_encoding_depth_no_types() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
        let result = cmd_check_encoding_depth(&path, false, 20, &DepthThresholds::default());
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_strict_threshold_triggers_failure() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "pub type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
        )
        .unwrap();
        // Set node threshold to 0 to force a violation.
        let thresholds = DepthThresholds {
            max_stack: 20,
            max_depth: 50,
            max_nodes: 0,
        };
        let result = cmd_check_encoding_depth(&path, false, 20, &thresholds);
        assert_eq!(result, ExitCode::FAILURE);
    }
}
