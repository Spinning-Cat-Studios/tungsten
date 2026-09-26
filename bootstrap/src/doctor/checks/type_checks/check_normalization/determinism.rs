//! Cross-run encoding-determinism check (ADR 22.7.26c close-out).
//!
//! `normalization-consistency` asks "does the stored encoding match a fresh
//! *re-derivation*?"; this check asks the narrower, complementary question
//! "does the driver produce the **same stored map twice**?". It elaborates
//! the project twice in-process and compares the two `encoded_types` maps
//! with strict structural `==` (deliberately NOT `normalize_for_comparison`
//! — the point is byte-stability of the stored form).
//!
//! Each `elaborate_project` call builds fresh `HashMap`s, and Rust's default
//! hasher seeds each map instance differently within a process, so the two
//! runs genuinely exercise different `env.types` iteration orders — the exact
//! axis ADR 22.7.26c made irrelevant to the stored output. For the strongest
//! cross-**process** guarantee, run with `--json` in two separate processes
//! and `diff` the outputs (a shared per-process hash seed cannot then hide a
//! flap); the in-process form is the fast canary.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::Type;

use crate::driver;

/// One type's cross-run comparison outcome.
struct Divergence {
    name: String,
    run_a: Option<String>,
    run_b: Option<String>,
}

/// The number of distinct type names across both runs (union size). Used as
/// the denominator so `total - divergences.len()` (stable count) never
/// underflows regardless of key disagreement between the runs.
fn union_len(run_a: &HashMap<String, Type>, run_b: &HashMap<String, Type>) -> usize {
    run_a.len()
        + run_b
            .keys()
            .filter(|name| !run_a.contains_key(*name))
            .count()
}

/// Compare two stored-encoding maps with strict structural `==`. Returns the
/// divergences (present-in-one, or unequal trees), sorted by name.
fn diff_encoding_maps(
    run_a: &HashMap<String, Type>,
    run_b: &HashMap<String, Type>,
) -> Vec<Divergence> {
    let mut divergences: Vec<Divergence> = Vec::new();
    for (name, a_encoding) in run_a {
        match run_b.get(name) {
            Some(b_encoding) if a_encoding == b_encoding => {}
            Some(b_encoding) => divergences.push(Divergence {
                name: name.clone(),
                run_a: Some(a_encoding.display_detailed()),
                run_b: Some(b_encoding.display_detailed()),
            }),
            None => divergences.push(Divergence {
                name: name.clone(),
                run_a: Some(a_encoding.display_detailed()),
                run_b: None,
            }),
        }
    }
    for name in run_b.keys() {
        if !run_a.contains_key(name) {
            divergences.push(Divergence {
                name: name.clone(),
                run_a: None,
                run_b: run_b.get(name).map(Type::display_detailed),
            });
        }
    }
    divergences.sort_by(|x, y| x.name.cmp(&y.name));
    divergences
}

/// Run `doctor check type encoding-determinism`.
///
/// Elaborates `file` twice and reports any stored-encoding divergence between
/// the two runs. Exits non-zero iff a divergence is found.
pub fn cmd_check_encoding_determinism(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    json: bool,
) -> ExitCode {
    let run_a = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output.encoded_types,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let run_b = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output.encoded_types,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let divergences = diff_encoding_maps(&run_a, &run_b);
    // Total = the union of type names across both runs, so
    // `total - divergences.len()` (the stable count) can never underflow even
    // when the runs disagree on which names exist (each side-only name is one
    // divergence, and both are in the union).
    let total = union_len(&run_a, &run_b);

    // The report body is built by pure, unit-tested formatters; this command
    // only does the I/O (elaborate ×2, print, exit). The `-> ExitCode`
    // Default-mutant on this thin wrapper is intentionally allowlisted: no test
    // can synthesize a *real* cross-run divergence on a healthy build (that is
    // exactly what ADR 22.7.26c guarantees), so an exit-code test could only
    // ever assert SUCCESS — the NormTally::exit precedent (see mod.rs).
    print!("{}", format_report(total, &divergences, verbose, json));
    if divergences.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Build the full report string for either output mode. Pure — unit-tested
/// both ways so the `json` dispatch and the content are asserted (the exit
/// code mapping in the caller is the only untestable residual).
fn format_report(total: usize, divergences: &[Divergence], verbose: bool, json: bool) -> String {
    if json {
        format_json(total, divergences)
    } else {
        format_human(total, divergences, verbose)
    }
}

/// The stable count: types present-and-equal in both runs.
fn stable_count(total: usize, divergences: &[Divergence]) -> usize {
    total - divergences.len()
}

fn format_human(total: usize, divergences: &[Divergence], verbose: bool) -> String {
    let mut out =
        format!("Checking {total} stored type encoding(s) for cross-run determinism...\n\n");
    for divergence in divergences {
        out.push_str(&format!(
            "  ✗ {}: NON-DETERMINISTIC across runs\n",
            divergence.name
        ));
        if verbose {
            match (&divergence.run_a, &divergence.run_b) {
                (Some(a), Some(b)) => {
                    out.push_str(&format!("    run A: {a}\n    run B: {b}\n"));
                }
                (a, b) => out.push_str(&format!(
                    "    present in run A: {}, in run B: {}\n",
                    a.is_some(),
                    b.is_some()
                )),
            }
        }
    }
    out.push_str(&format!(
        "\nResult: {} non-deterministic, {} stable (strict structural ==)\n",
        divergences.len(),
        stable_count(total, divergences)
    ));
    out
}

fn format_json(total: usize, divergences: &[Divergence]) -> String {
    let entries: Vec<serde_json::Value> = divergences
        .iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "run_a": d.run_a,
                "run_b": d.run_b,
            })
        })
        .collect();
    let report = serde_json::json!({
        "total": total,
        "non_deterministic": divergences.len(),
        "stable": stable_count(total, divergences),
        "divergences": entries,
    });
    serde_json::to_string_pretty(&report).unwrap()
}

// Tests: determinism_tests.rs
#[cfg(test)]
#[path = "determinism_tests.rs"]
mod determinism_tests;
