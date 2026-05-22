//! Type-term size check (ADR 20.4.26d).
//!
//! Reports node counts for all type encodings, sorted by size.
//! Flags types exceeding a configurable threshold.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;

/// Default threshold for maximum type-term node count.
const DEFAULT_MAX_NODES: usize = 5000;

/// Run the type-term size check.
///
/// Elaborates the project and reports the node count of every cached
/// type encoding, sorted from largest to smallest.
pub fn cmd_check_type_sizes(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    max_nodes: usize,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let encoded_types = &project.encoded_types;

    if encoded_types.is_empty() {
        println!("No cached type encodings found.");
        return ExitCode::SUCCESS;
    }

    // Collect metrics.
    let mut entries: Vec<(String, usize, usize)> = encoded_types
        .iter()
        .map(|(name, ty)| (name.clone(), ty.node_count(), ty.depth()))
        .collect();

    // Sort by node count descending.
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    println!("Type Encoding Sizes");
    println!("═══════════════════");
    println!();
    println!("{:>6}  {:>5}  Name", "Nodes", "Depth");
    println!("{:>6}  {:>5}  ────", "─────", "─────");

    let mut violations = 0;

    for (name, nodes, depth) in &entries {
        let flag = if *nodes > max_nodes {
            " ← EXCEEDS"
        } else {
            ""
        };
        println!("{nodes:>6}  {depth:>5}  {name}{flag}");
        if *nodes > max_nodes {
            violations += 1;
        }
    }

    println!();
    println!(
        "Total: {} type(s), threshold: {} nodes",
        entries.len(),
        max_nodes
    );

    if verbose {
        // Show summary statistics.
        let total_nodes: usize = entries.iter().map(|(_, n, _)| n).sum();
        let avg_nodes = total_nodes / entries.len().max(1);
        let max_depth = entries.iter().map(|(_, _, d)| d).max().unwrap_or(&0);
        println!();
        println!("Summary:");
        println!("  Total nodes across all types: {total_nodes}");
        println!("  Average nodes per type: {avg_nodes}");
        println!("  Max depth: {max_depth}");
    }

    if violations == 0 {
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("{violations} type(s) exceed the node count threshold of {max_nodes}");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_check_type_sizes_simple() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "pub type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_type_sizes(&path, false, 20, DEFAULT_MAX_NODES);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_check_type_sizes_no_types() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(&path, "fn main() -> Nat { 0 }").unwrap();
        let result = cmd_check_type_sizes(&path, false, 20, DEFAULT_MAX_NODES);
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
        let result = cmd_check_type_sizes(&path, false, 20, 0);
        assert_eq!(result, ExitCode::FAILURE);
    }
}
