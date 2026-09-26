//! Rendering for `tungsten info module dependents` (ADR 5.9.26f).
//!
//! Kept apart from the inversion so the shape of the answer is assertable
//! without a module tree: every function here is pure over a
//! [`DependentsReport`].

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{Dependent, DependentsReport};

/// Render the report.
///
/// **`0 dependents` and `0 modules read` must not render alike** (ADR 5.9.26f
/// AC3). An empty tree is a fault in this command — a parsed tree holds at
/// least its root — so it says so instead of printing a tidy zero over a
/// corpus nothing read.
#[must_use]
pub fn render_report(report: &DependentsReport, verbose: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Dependents of module '{}'\n", report.module);
    if report.modules_in_tree == 0 {
        let _ = writeln!(
            out,
            "** no modules read — this is a FAULT in `info module dependents`, not a \
             finding: a parsed module tree holds at least its root. **"
        );
        return out;
    }
    render_direct(&mut out, report);
    render_indirect(&mut out, report, verbose);
    render_literals(&mut out, report);
    let _ = writeln!(
        out,
        "reach: {} module(s) in tree, {} import site(s) examined, {} unresolved",
        report.modules_in_tree, report.sites_examined, report.sites_unresolved
    );
    out
}

fn render_direct(out: &mut String, report: &DependentsReport) {
    let _ = writeln!(
        out,
        "direct ({}) — name this module in their path; a move changes these",
        report.direct.len()
    );
    for dep in &report.direct {
        let _ = writeln!(out, "  {}:{}  {}", dep.file.display(), dep.line, dep.text);
    }
    let _ = writeln!(out);
}

fn render_indirect(out: &mut String, report: &DependentsReport, verbose: bool) {
    let _ = writeln!(
        out,
        "indirect ({}) — reach these items through a re-export; a move does not touch them",
        report.indirect.len()
    );
    let mut by_hop: BTreeMap<String, Vec<&Dependent>> = BTreeMap::new();
    for dep in &report.indirect {
        let hop = dep
            .via
            .as_ref()
            .map_or_else(|| "?".to_string(), ToString::to_string);
        by_hop.entry(hop).or_default().push(dep);
    }
    for (hop, deps) in &by_hop {
        let _ = writeln!(out, "  via {hop} — {} site(s)", deps.len());
        if !verbose {
            continue;
        }
        for dep in deps {
            let _ = writeln!(out, "    {}:{}  {}", dep.file.display(), dep.line, dep.text);
        }
    }
    if !by_hop.is_empty() && !verbose {
        let _ = writeln!(out, "  … --verbose to list the sites");
    }
    let _ = writeln!(out);
}

fn render_literals(out: &mut String, report: &DependentsReport) {
    let _ = writeln!(
        out,
        "literals ({}) — text matches in .tg string literals, not resolved references",
        report.literals.len()
    );
    for lit in &report.literals {
        let _ = writeln!(
            out,
            "  {}:{}  \"{}\"",
            lit.file.display(),
            lit.line,
            lit.text
        );
    }
    let _ = writeln!(out);
}
