//! Phase invariant checking doctor subcommand (ADR 20.4.26e).
//!
//! Runs the elaboration pipeline with invariant checks at each phase
//! boundary and reports results.

use std::path::PathBuf;
use std::process::ExitCode;

/// Run the phase invariant checker on a source file.
pub fn cmd_check_phase_invariants(file: &PathBuf, verbose: bool, _max_errors: usize) -> ExitCode {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", file.display());
            return ExitCode::FAILURE;
        }
    };

    let (ast, parse_errors) = crate::parse(&source);
    if !parse_errors.is_empty() {
        eprintln!(
            "error: {} parse error(s) in {}",
            parse_errors.len(),
            file.display()
        );
        for e in &parse_errors {
            eprintln!("  {e}");
        }
        return ExitCode::FAILURE;
    }

    println!("Running elaboration with invariant checking...");
    println!();

    let mut ctx = tungsten_core::Context::new();
    let (phase_results, elab_result) =
        crate::elaborate::elaborate_with_phase_checks(&ast, &mut ctx);

    // Report each phase result
    let mut any_failed = false;
    for result in &phase_results {
        let mark = if result.passed { "✓" } else { "✗" };
        println!("[{}] {} {}", result.phase, mark, result.stats);

        if !result.passed {
            any_failed = true;
            println!("  INVARIANT VIOLATION:");
            for violation in &result.violations {
                println!("    {violation}");
            }
            println!();
        } else if verbose {
            // In verbose mode, show stats even for passing phases
        }
    }

    // Report elaboration outcome
    println!();
    match elab_result {
        Ok(_) => {
            if verbose {
                println!("Elaboration succeeded.");
            }
        }
        Err(errors) => {
            println!(
                "Note: elaboration produced {} error(s) (invariant checks still ran).",
                errors.len()
            );
            if verbose {
                for e in &errors {
                    println!("  {e}");
                }
            }
        }
    }

    if any_failed {
        let failed_count = phase_results.iter().filter(|r| !r.passed).count();
        println!(
            "Phase invariant check FAILED ({} phase(s) with violations).",
            failed_count
        );
        ExitCode::FAILURE
    } else {
        println!("All phase invariants hold.");
        ExitCode::SUCCESS
    }
}
