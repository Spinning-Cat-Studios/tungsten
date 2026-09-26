//! Normalization consistency check (ADR 20.4.26c).
//!
//! Compares the driver's cached type encodings (Encoding Finalization) against an
//! independently produced set, reporting divergences.
//!
//! Two comparison modes:
//! - **Single-module entry** (ADR 21.7.26e tooling-gap fix): re-elaborates the
//!   file standalone, comparing the driver pipeline's encodings against the
//!   direct elaboration path.
//! - **Multi-module entry** (`mod` declarations, ADR 21.7.26j): standalone
//!   re-elaboration cannot resolve cross-module imports, so the check runs a
//!   **live-elaborator normalization comparison** — it seeds a fresh
//!   whole-project `Elaborator` from the Phase-B exports (via
//!   `driver::elaborate_project_with_inspector`) and asserts, for each stored
//!   Phase-1e encoding, that `normalize_for_comparison(App(name, params))`
//!   equals it (the §2.1 `encode(fresh) ≡ₙ stored` invariant). It falls back
//!   to a cross-run determinism comparison only if the driver hook fails to
//!   elaborate. Previously this case hard-errored with "failed to
//!   re-elaborate", then (21.7.26e) shipped only the cross-run fallback.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::Type;

use crate::driver;
use crate::elaborate::ProjectNormalizer;

mod cross_run;
use cross_run::{
    compare_stored_vs_fresh, cross_run_fallback, format_cross_encoding_result,
    single_module_fresh_encodings,
};

mod determinism;
pub use determinism::cmd_check_encoding_determinism;

mod resolution_attempts;
pub use resolution_attempts::cmd_check_resolution_attempt_determinism;

/// Run the normalization consistency check.
///
/// For each type with a Phase-1e encoding from the primary driver run,
/// compares it against an independently produced form and reports divergences.
/// Single-module entries use a standalone re-elaboration; multi-module entries
/// use the live-elaborator normalization comparison (ADR 21.7.26j), falling
/// back to a second driver run only if the driver hook fails to elaborate.
///
/// `raw_only` (ADR 22.7.26c) drops the multi-module per-module-oracle tier-2
/// normalization fallback, comparing with raw structural `==` alone. Since
/// ADR 22.7.26d fixed the Phase-1d resolution order, tier-1 suffices on
/// healthy code (main.tg: 0 divergent raw-only, cross-process stable), so the
/// flag is a regression canary for the retired inline-depth instability
/// rather than a way to see past a load-bearing tier 2. It has no effect on
/// the single-module path, which never had a tier-2 step.
pub fn cmd_check_normalization_consistency(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    raw_only: bool,
) -> ExitCode {
    // Single-module entry: compare the driver encodings against a standalone
    // re-elaboration (ADR 21.7.26e). Detected cheaply — the standalone attempt
    // fails fast on a multi-module entry's unresolved imports.
    if let Some(fresh) = single_module_fresh_encodings(file) {
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
        let tally = compare_stored_vs_fresh(&project.encoded_types, &type_names, &fresh, verbose);
        // The shared format is `format_cross_encoding_result` (one tested place);
        // the `println!` + `NormTally::exit` stay inline here (and in
        // `cross_run_fallback`) rather than a shared `-> ExitCode` helper, whose
        // `-> Default` mutant is unkillable — no test synthesizes a real divergence.
        println!("{}", format_cross_encoding_result(&tally));
        return tally.exit();
    }

    // Multi-module entry: the single-pass live-elaborator normalization
    // comparison (ADR 21.7.26j). It internally falls back to the cross-run
    // determinism check only if the driver hook itself fails to elaborate.
    live_multi_module_check(file, max_errors, CompareOpts { verbose, raw_only })
}

/// Print the run header (or the empty-set note) and return the sorted type
/// names. `None` means there were no cached encodings — the caller returns
/// success without comparing.
fn print_header_or_empty(encoded_types: &HashMap<String, Type>) -> Option<Vec<&str>> {
    if encoded_types.is_empty() {
        println!("No cached type encodings found.");
        return None;
    }
    let mut type_names: Vec<&str> = encoded_types.keys().map(|s| s.as_str()).collect();
    type_names.sort();
    println!(
        "Checking {} type encoding(s) for normalization consistency...",
        type_names.len()
    );
    println!();
    Some(type_names)
}

/// Consistent / skipped / divergent counts from a comparison pass. Returned by
/// the comparison functions (instead of a bare `ExitCode`) so tests can assert
/// the actual tally, not just the exit code (ADR 21.7.26j close-out).
#[derive(Debug, Default, PartialEq, Eq)]
struct NormTally {
    consistent: usize,
    skipped: usize,
    divergent: usize,
}

impl NormTally {
    /// Exit non-zero iff a divergence was found.
    fn exit(&self) -> ExitCode {
        if self.divergent > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        }
    }
}

/// Whether `ty` contains any unexpanded `Type::App(...)` node.
///
/// `normalize_for_comparison` leaves record references nominal, and — for a
/// generic instantiation like `List<PathSeg>` / `Option<X>` in a stored alias
/// or field — leaves the `App(Generic, [args])` node **unexpanded**, whereas
/// the stored Phase-1e encoding fully expands both (`List<PathSeg>` → `Mu(…)`,
/// `Option<X>` → `Sum(…)`). When the fresh comparand still holds an `App`, it is
/// not a faithful reproduction of the stored structural encoding, so the type
/// is reported as a *skip* rather than a divergence: the normalize-vs-Phase-1e
/// fidelity limit (ADR 21.7.26j §3), which is exactly the wall-1 generic type
/// class the per-module-oracle follow-up would lift.
fn type_contains_unexpanded_app(ty: &Type) -> bool {
    matches!(ty, Type::App(_, _))
        || ty
            .children()
            .iter()
            .any(|child| type_contains_unexpanded_app(child))
}

/// One type's live-comparison outcome.
#[derive(Debug, PartialEq, Eq)]
enum LiveVerdict {
    Consistent,
    Skipped,
    Divergent,
}

/// Options controlling a live normalization comparison (the per-type
/// classifiers' shared knobs, extracted to keep their arity ≤ 5).
#[derive(Clone, Copy)]
struct CompareOpts {
    /// Print a per-type line for every outcome, not just divergences.
    verbose: bool,
    /// Skip the tier-2 normalization fallback (`--raw-only`, ADR 22.7.26c):
    /// compare with raw `==` alone, surfacing the record-body inline-depth
    /// residual (ADR 22.7.26d) that tier-2 otherwise absorbs.
    raw_only: bool,
}

/// Tally a sequence of per-type verdicts. Pure — unit-tested directly so the
/// counting is asserted, not just the exit code (ADR 21.7.26j close-out).
fn tally_verdicts(verdicts: &[LiveVerdict]) -> NormTally {
    let mut tally = NormTally::default();
    for verdict in verdicts {
        match verdict {
            LiveVerdict::Consistent => tally.consistent += 1,
            LiveVerdict::Skipped => tally.skipped += 1,
            LiveVerdict::Divergent => tally.divergent += 1,
        }
    }
    tally
}

/// Classify one type's stored encoding, printing the divergence detail (and,
/// under `verbose`, the per-type line).
///
/// Primary comparison (ADR 22.7.26b): the per-module oracle's source-fresh
/// Phase-1e encoding — its defining module's collection pass re-run with the
/// whole-project exports injected. Both sides are full Phase-1e encodings
/// (records and generic instantiations fully expanded), so derived structural
/// `==` is the exact `encode(fresh) ≡ stored` invariant with no skip class.
///
/// Fallback (ADR 21.7.26j): for names with no per-module view (e.g. a
/// dependency's types outside the parsed tree), the live-elaborator
/// normalization comparison, which skips under-expanded comparands.
fn classify_type_live(
    normalizer: &ProjectNormalizer,
    name: &str,
    stored: &Type,
    opts: CompareOpts,
) -> LiveVerdict {
    if let Some(fresh) = normalizer.per_module_fresh(name) {
        return classify_against_per_module_fresh(normalizer, name, stored, fresh, opts);
    }
    let Some(params) = normalizer.type_params(name) else {
        if opts.verbose {
            println!("  ? {name}: not in project type env (skipped)");
        }
        return LiveVerdict::Skipped;
    };
    // The fresh spelling: the type applied to its own formal params as free
    // TyVars — `encode(fresh)` in the §2.1 invariant.
    let fresh_spelling = Type::App(
        name.to_string(),
        params.iter().map(|p| Type::TyVar(p.clone())).collect(),
    );
    let fresh_normalized = normalizer.normalize(&fresh_spelling);
    let stored_normalized = normalizer.normalize(stored);
    if stored_normalized == fresh_normalized {
        if opts.verbose {
            println!("  ✓ {name}: consistent (live)");
        }
        LiveVerdict::Consistent
    } else if type_contains_unexpanded_app(&fresh_normalized)
        || type_contains_unexpanded_app(&stored_normalized)
    {
        // The fresh comparand still holds an unexpanded `App` (a record kept
        // nominal, or a generic instantiation `normalize` under-expands vs
        // Phase-1e) → not a faithful reproduction of the stored encoding, so the
        // comparison is inconclusive: skip, not divergence (§3 gates (a)/(c)).
        if opts.verbose {
            println!(
                "  ? {name}: comparand under-expanded (record / generic instantiation) — skipped"
            );
        }
        LiveVerdict::Skipped
    } else {
        // A genuine structural divergence not explained by under-expansion —
        // the wall-1 detector firing.
        println!("  ✗ {name}: DIVERGENT (live-elaborator normalization)");
        println!(
            "    Stored (normalized):  {}",
            stored_normalized.display_detailed()
        );
        println!(
            "    Fresh  (normalized):  {}",
            fresh_normalized.display_detailed()
        );
        println!();
        LiveVerdict::Divergent
    }
}

/// Compare a stored encoding against the per-module oracle's source-fresh
/// Phase-1e re-derivation (ADR 22.7.26b). Two tiers:
///
/// 1. Plain structural `==` (which handles `Adt` correctly) — the expected
///    path, and since ADR 22.7.26d it suffices on healthy code: both sides
///    resolve Deferred-TyVar Resolution in the same deterministic, dependency-respecting
///    order (ADR 22.7.26c's Phase-1e order, extended to 1d with
///    inline-relevant edge filtering), so stored and fresh converge on the
///    same maximally-inlined trees (main.tg: 0 divergent raw-only,
///    cross-process stable).
/// 2. On raw mismatch, compare both sides normalized through the same project
///    env. **Defense-in-depth only** (ADR 22.7.26d demotion): the
///    record-body inline-depth instability this tier used to absorb —
///    1–4 record entries (MatchArm, FieldInit, PatternInfo, TypeDef)
///    flapping between a nominal `TyVar(@Path)` and its full expansion on
///    the fresh side — was fixed at the source (hash-order Phase-1d
///    resolution + the nominal-back-edge SCC weld; see
///    `dependency_respecting_type_order`). Tier 2 remains as a guard for
///    per-module-env vs whole-project-env residue and future skew; genuine
///    structural corruption (a wrong field type, a swapped Sum arm) stays
///    divergent through it.
fn classify_against_per_module_fresh(
    normalizer: &ProjectNormalizer,
    name: &str,
    stored: &Type,
    fresh: &Type,
    opts: CompareOpts,
) -> LiveVerdict {
    // Tier 1: raw structural `==` — sufficient on healthy code since the
    // Phase-1d resolution order became deterministic (ADR 22.7.26d). Tier 2
    // (skipped under `raw_only`): normalize both sides through the shared
    // project env — defense-in-depth only, guarding env-shape residue and
    // future skew, no longer absorbing a known instability.
    let tier2_ok = !opts.raw_only && normalizer.normalize(stored) == normalizer.normalize(fresh);
    if stored == fresh || tier2_ok {
        if opts.verbose {
            println!("  ✓ {name}: consistent (per-module oracle)");
        }
        LiveVerdict::Consistent
    } else {
        println!("  ✗ {name}: DIVERGENT (per-module oracle)");
        println!(
            "    Stored (Encoding Finalization):    {}",
            stored.display_detailed()
        );
        println!("    Fresh  (re-collect):  {}", fresh.display_detailed());
        println!();
        LiveVerdict::Divergent
    }
}

/// The single-pass live-elaborator normalization comparison for multi-module
/// trees (ADR 21.7.26j).
///
/// One project elaboration seeds a fresh whole-project `Elaborator` (carrying
/// that run's stored Phase-1e encodings); [`classify_type_live`] compares each
/// stored encoding against `normalize_for_comparison(App(name, params))` — the
/// §2.1 `encode(fresh) ≡ₙ stored` invariant. If the driver hook fails to
/// elaborate, falls back internally to the cross-run determinism comparison.
fn live_multi_module_check(file: &PathBuf, max_errors: usize, opts: CompareOpts) -> ExitCode {
    let mut verdicts: Vec<LiveVerdict> = Vec::new();
    let mut empty = false;

    let result = driver::elaborate_project_with_inspector(
        file,
        opts.verbose,
        max_errors,
        &mut |normalizer| {
            let encoded_types = normalizer.stored_encodings();
            if encoded_types.is_empty() {
                println!("No cached type encodings found.");
                empty = true;
                return;
            }
            let mut type_names: Vec<&str> = encoded_types.keys().map(|s| s.as_str()).collect();
            type_names.sort();
            println!(
                "Checking {} type encoding(s) for normalization consistency...",
                type_names.len()
            );
            let tier_note = if opts.raw_only {
                " [--raw-only: tier-2 normalization disabled]"
            } else {
                ""
            };
            println!(
                "note: multi-module entry — per-module oracle (ADR 22.7.26b), live-elaborator fallback (ADR 21.7.26j).{tier_note}"
            );
            println!();

            verdicts = type_names
                .iter()
                .map(|&name| classify_type_live(normalizer, name, &encoded_types[name], opts))
                .collect();
        },
    );

    // Elaboration failed → fall back to the cross-run determinism comparison.
    if result.is_err() {
        eprintln!(
            "note: live-elaborator hook failed to elaborate; falling back to cross-run comparison."
        );
        return cross_run_fallback(file, opts.verbose, max_errors);
    }

    if empty {
        return ExitCode::SUCCESS;
    }

    let tally = tally_verdicts(&verdicts);
    println!();
    println!(
        "Result: {} divergent, {} consistent, {} skipped (per-module oracle, ADR 22.7.26b)",
        tally.divergent, tally.consistent, tally.skipped
    );
    tally.exit()
}
// Tests: tests.rs (pre-oracle check paths), oracle_tests.rs (ADR 22.7.26b)
#[cfg(test)]
#[path = "oracle_tests.rs"]
mod oracle_tests;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
