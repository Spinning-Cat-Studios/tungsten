//! `tungsten doctor check type positivity <file>` — strict-positivity report
//! (ADR 7.8.26e §2.4).
//!
//! Read-only counterpart of the E0061 gate: it runs the *same* engine over the
//! same input (`elaborate::positivity::analyze`), so the tool and the gate
//! cannot disagree about whether a type is strictly positive. What it adds is
//! the data the gate does not surface — each parameter's computed
//! `Occ`, the SCC sizes, the Tarjan depth, and the unresolved-head census split
//! by `Stub` vs genuinely-absent.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::driver;
use crate::elaborate::positivity::{self, PositivityReport};

/// The verdict, separated from printing so it is assertable.
#[derive(Debug, PartialEq, Eq)]
pub struct PositivityTally {
    /// Definitions checked (every ADT and record, aliases expanded away).
    pub definitions: usize,
    /// Distinct types with at least one violation — D6's *V*.
    pub violating_types: usize,
    /// Total violations (a type can violate in several fields).
    pub violations: usize,
    /// Largest SCC in the expanded type graph.
    pub max_group_size: usize,
    /// Deepest `tarjan_scc` recursion.
    pub max_tarjan_depth: usize,
    /// `App`/`Adt` heads naming a lossy stub.
    pub stub_heads: usize,
    /// `App`/`Adt` heads absent from the definition map entirely.
    pub unknown_heads: usize,
}

impl PositivityTally {
    /// Summarize one report.
    #[must_use]
    pub fn from_report(report: &PositivityReport) -> Self {
        let mut violating: Vec<&str> = report
            .violations
            .iter()
            .map(|v| v.type_name.as_str())
            .collect();
        violating.sort_unstable();
        violating.dedup();
        PositivityTally {
            definitions: report.param_occs.len(),
            violating_types: violating.len(),
            violations: report.violations.len(),
            max_group_size: report.max_group_size(),
            max_tarjan_depth: report.max_tarjan_depth,
            stub_heads: report.census.stub.len(),
            unknown_heads: report.census.unknown.len(),
        }
    }

    /// Non-zero exactly when a type is not strictly positive.
    #[must_use]
    pub fn exit(&self) -> ExitCode {
        if self.violating_types == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

/// Entry point for `tungsten doctor check type positivity <file>`.
pub fn cmd_check_positivity(file: &PathBuf, verbose: bool, max_errors: usize) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let (defs, spans) = positivity::from_project(&project);
    let report = positivity::analyze(&defs, spans);
    let tally = PositivityTally::from_report(&report);

    print!("{}", render_report(&report, &tally, verbose));
    tally.exit()
}

/// The command's whole output, as a value.
///
/// Building the text rather than printing it keeps every branch here — the
/// clean/violating split, the `via` rendering, the three `--verbose` sections —
/// assertable from a unit test. `cmd_check_positivity` is then thin enough to
/// be uninteresting: elaborate, build, analyze, tally, print, exit.
fn render_report(report: &PositivityReport, tally: &PositivityTally, verbose: bool) -> String {
    let mut out = String::new();
    if tally.violating_types == 0 {
        out.push_str(&format!(
            "✓ {} definition(s) strictly positive (max SCC {}, Tarjan depth {})\n",
            tally.definitions, tally.max_group_size, tally.max_tarjan_depth
        ));
    } else {
        out.push_str(&format!(
            "✗ {} type(s) not strictly positive ({} violation(s)):\n",
            tally.violating_types, tally.violations
        ));
        for violation in &report.violations {
            let via: String = violation
                .via
                .iter()
                .map(|link| format!(" via `{}`<{}>", link.type_name, link.param))
                .collect();
            out.push_str(&format!(
                "  {}.{} {} — `{}` at a forbidden position{via}\n",
                violation.type_name, violation.ctor_name, violation.field, violation.occurrence
            ));
        }
    }

    out.push_str(&format!(
        "  unresolved heads: {} stub, {} unknown\n",
        tally.stub_heads, tally.unknown_heads
    ));

    if !verbose {
        return out;
    }
    out.push_str("\nParameter strictness:\n");
    for (name, occs) in &report.param_occs {
        if occs.is_empty() {
            continue;
        }
        let rendered: Vec<&str> = occs.iter().map(|occ| occ.label()).collect();
        out.push_str(&format!("  {name}<{}>\n", rendered.join(", ")));
    }
    if !report.census.stub.is_empty() {
        out.push_str(&format!(
            "\nStub heads (skipped): {}\n",
            join(&report.census.stub)
        ));
    }
    if !report.census.unknown.is_empty() {
        out.push_str(&format!(
            "\nUnknown heads (doubted): {}\n",
            join(&report.census.unknown)
        ));
    }
    out
}

fn join(names: &std::collections::BTreeSet<String>) -> String {
    names.iter().cloned().collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests;
