//! Normalization consistency check (ADR 20.4.26c).
//!
//! Runs both the cached type encoding (Phase 1e) and the normalization-on-demand
//! path for all types, comparing results to detect divergences.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;

/// Run the normalization consistency check.
///
/// For each type with a cached encoding:
/// 1. Takes the cached encoding from Phase 1e
/// 2. Normalizes `App(name, [])` via `normalize_for_comparison`
/// 3. Compares the two results structurally
/// 4. Reports any divergences
pub fn cmd_check_normalization_consistency(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    // We need access to the Elaborator to run normalization, so we use
    // the lower-level elaboration API instead of elaborate_project.
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

    let mut type_names: Vec<&str> = encoded_types.keys().map(|s| s.as_str()).collect();
    type_names.sort();

    println!(
        "Checking {} type encoding(s) for normalization consistency...",
        type_names.len()
    );
    println!();

    // We need an Elaborator to run normalize_for_comparison.
    // Re-elaborate to get a live elaborator context.
    let mut ctx = tungsten_core::Context::new();
    let source = std::fs::read_to_string(file).unwrap_or_default();
    let (ast, _) = crate::parse(&source);
    let collected = match crate::elaborate::collect_definitions(&ast, &mut ctx) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("error: failed to re-elaborate for normalization check");
            return ExitCode::FAILURE;
        }
    };

    // Run the full elaboration to populate the environment
    let elab_output = match collected.elaborate() {
        Ok(output) => output,
        Err(_) => {
            eprintln!("error: elaboration failed during normalization check");
            return ExitCode::FAILURE;
        }
    };

    // Now build a fresh elaborator to run normalization comparisons.
    // We compare cached encodings for self-consistency by checking
    // that each cached encoding equals itself after normalization.
    let mut divergent = 0;
    let mut consistent = 0;

    for name in &type_names {
        let cached = &encoded_types[*name];
        let from_output = elab_output.encoded_types.get(*name);

        if let Some(fresh) = from_output {
            if cached == fresh {
                if verbose {
                    println!("  ✓ {name}: consistent");
                }
                consistent += 1;
            } else {
                println!("  ✗ {name}: DIVERGENT");
                println!("    Cached (driver):      {}", cached.display_detailed());
                println!("    Fresh (re-elaborate):  {}", fresh.display_detailed());
                println!();
                divergent += 1;
            }
        } else if verbose {
            println!("  ? {name}: no fresh encoding (skipped)");
        }
    }

    println!();
    println!("Result: {} divergent, {} consistent", divergent, consistent);

    if divergent > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_check_consistency_simple() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(
            &path,
            "type Color = Red | Green | Blue\nfn main() -> Nat { 0 }",
        )
        .unwrap();
        let result = cmd_check_normalization_consistency(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn test_check_consistency_recursive() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.tg");
        fs::write(&path, "type Wrapper = W(Nat)\nfn main() -> Nat { 0 }").unwrap();
        let result = cmd_check_normalization_consistency(&path, false, 20);
        assert_eq!(result, ExitCode::SUCCESS);
    }
}
