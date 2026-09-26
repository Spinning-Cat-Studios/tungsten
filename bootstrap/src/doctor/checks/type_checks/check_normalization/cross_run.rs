//! The non-live comparison paths of the normalization check: the single-module
//! standalone re-elaboration (ADR 21.7.26e) and the multi-module cross-run
//! determinism fallback. Split out of `mod.rs` (file-size paydown); the live
//! multi-module comparison stays in `mod.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::Type;

use super::{print_header_or_empty, NormTally};
use crate::driver;

/// The `Result:` summary block for the cross-encoding-set comparison
/// (single-module + cross-run share this exact wording). Pure + unit-tested so
/// the format lives in one asserted place; callers `println!` it and call
/// `NormTally::exit` **inline** rather than through a shared `-> ExitCode`
/// helper, whose `-> Default` mutant would be unkillable (no test can
/// synthesize a real stored-vs-fresh divergence, so it only ever returns
/// SUCCESS). ADR 21.7.26j close-out.
pub(super) fn format_cross_encoding_result(tally: &NormTally) -> String {
    format!(
        "\nResult: {} divergent, {} consistent",
        tally.divergent, tally.consistent
    )
}

/// Fresh encodings produced independently of the primary driver run, plus a
/// label describing which path produced them (shown in divergence output).
pub(super) struct FreshEncodings {
    pub(super) label: &'static str,
    pub(super) encoded_types: HashMap<String, Type>,
}

/// Re-elaborate a single-module file standalone. `None` when the file cannot
/// be elaborated this way (multi-module entry, parse errors, etc.).
pub(super) fn single_module_fresh_encodings(file: &PathBuf) -> Option<FreshEncodings> {
    let source = std::fs::read_to_string(file).ok()?;
    let (ast, parse_errors) = crate::parse(&source);
    if !parse_errors.is_empty() {
        return None;
    }
    let mut ctx = tungsten_core::Context::new();
    let collected = crate::elaborate::collect_definitions(&ast, &mut ctx).ok()?;
    let elab_output = collected.elaborate().ok()?;
    Some(FreshEncodings {
        label: "re-elaborate (single-module)",
        encoded_types: elab_output.encoded_types,
    })
}

/// Run the full driver pipeline a second time (the multi-module fallback).
fn second_driver_run_encodings(file: &PathBuf, max_errors: usize) -> Option<FreshEncodings> {
    let second = driver::elaborate_project(file, false, max_errors, None).ok()?;
    Some(FreshEncodings {
        label: "second driver elaboration",
        encoded_types: second.encoded_types,
    })
}

/// The cross-run determinism fallback for a multi-module entry whose live
/// driver hook failed: a second full driver elaboration compared against the
/// first (weaker signal, but real).
pub(super) fn cross_run_fallback(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(type_names) = print_header_or_empty(&project.encoded_types) else {
        return ExitCode::SUCCESS;
    };
    println!("note: run `tungsten cache clean` first for a fully cache-independent comparison.");
    println!();
    let Some(fresh) = second_driver_run_encodings(file, max_errors) else {
        eprintln!("error: second driver elaboration failed during normalization check");
        return ExitCode::FAILURE;
    };
    let tally = compare_stored_vs_fresh(&project.encoded_types, &type_names, &fresh, verbose);
    println!("{}", format_cross_encoding_result(&tally));
    tally.exit()
}

/// Compare each stored encoding against an independently produced encoding set
/// (single-module re-elaboration, or the cross-run fallback), printing per-type
/// detail and returning the tally.
pub(super) fn compare_stored_vs_fresh(
    encoded_types: &HashMap<String, Type>,
    type_names: &[&str],
    fresh: &FreshEncodings,
    verbose: bool,
) -> NormTally {
    let mut tally = NormTally::default();

    for name in type_names {
        let cached = &encoded_types[*name];
        if let Some(fresh_encoding) = fresh.encoded_types.get(*name) {
            if cached == fresh_encoding {
                if verbose {
                    println!("  ✓ {name}: consistent");
                }
                tally.consistent += 1;
            } else {
                println!("  ✗ {name}: DIVERGENT");
                println!("    Cached (driver):      {}", cached.display_detailed());
                println!(
                    "    Fresh ({}):  {}",
                    fresh.label,
                    fresh_encoding.display_detailed()
                );
                println!();
                tally.divergent += 1;
            }
        } else {
            if verbose {
                println!("  ? {name}: no fresh encoding (skipped)");
            }
            tally.skipped += 1;
        }
    }
    tally
}
