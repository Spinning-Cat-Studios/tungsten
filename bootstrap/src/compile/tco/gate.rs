//! The `tco-coverage --gate` deterministic CI gate (ADR 1.7.26e §5, R7/R8).
//!
//! Turns the musttail inventory into a pass/fail gate: **fail** on any HIGH-risk
//! `SKIP` that is not in a checked-in allowlist, *and* on any allowlist entry
//! that participates in internal recursion. Deterministic — unlike a
//! `ulimit`-sensitive overflow test.
//!
//! ## Why the allowlist is guarded (and usually inert)
//!
//! `tco-coverage` ranks **self-recursive** functions, so every function it can
//! report as a `SKIP` is by construction recursion-participating. Per ADR
//! 1.7.26e R8 a recursion-participating `SKIP` is a **hard failure even if
//! allowlisted** — such a function MUST reach EMIT/DECOMPOSE. The allowlist may
//! therefore only legitimately hold **non-recursion-participating** boundary
//! shapes (genuine varargs / extern edges that can never be a `musttail` edge),
//! which the self-recursive inventory never lists. So in practice the gate is
//! "zero HIGH-risk `SKIP`", and the allowlist is a documented, guarded escape
//! hatch: listing a self-recursive `SKIP` there does **not** excuse it — the gate
//! rejects the entry as recursion-participating.
//!
//! ## What this gate does NOT see (ADR 5.8.26a)
//!
//! **Mutual** tail recursion. ADR 17.7.26d parked mutual-tail Class-P `musttail`
//! on the rationale that this gate made it self-guarding — *"the R8 gate fails
//! on a recursion-participating `SKIP`, so it cannot land silently."* That was
//! false in two independent ways, and only one of them has been fixed here.
//!
//! It was false because the gate ran nowhere: fully implemented, described by
//! `info pipeline` as "the deterministic CI form", and invoked by no `make`
//! target and no workflow. That half is fixed — `make check-tco-gate` runs it in
//! CI's `build-with-llvm` job.
//!
//! It was also false because a mutual edge produced no row to gate on: the
//! non-self-recursive branch of `try_emit_saturated_musttail` was trace-only, so
//! an `f→g→f` Class-P cycle contributed zero [`FunctionCoverage`] rows and this
//! gate printed `✓` over a row set that could not contain it. Those edges are
//! now recorded as [`Decision::SkipNonSelf`] and counted
//! (`risk::non_self_tail_sites`, surfaced in the table and in `--json`
//! `totals.skip_non_self`) — **but they are still not judged here**, and that is
//! deliberate. Deciding whether an edge closes a recursion cycle needs the
//! recursion call graph, which codegen does not have (`doctor audit-recursion`
//! computes one; joining the two is the unbuilt prerequisite). Gating on the
//! unclassified population instead would fail on all 1,286 of them, nearly all
//! benign — and a gate that is red over a case nobody can act on is one people
//! learn to ignore, which is the failure this gate exists to prevent.
//!
//! So: `✓` from this gate means *"no un-allowlisted HIGH-risk self-recursive
//! SKIP"*. It does not mean the compiler is free of O(N)-stack mutual recursion.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tungsten_codegen::Decision;

use super::risk::{FunctionCoverage, Risk};

/// The checked-in allowlist location, relative to the repo root (discovered by
/// walking up from CWD, mirroring `tools/code-health/`).
pub(crate) const ALLOWLIST_REL_PATH: &str = "tools/tco-skip-allowlist.toml";

/// Current allowlist schema version.
const SCHEMA_VERSION: u32 = 1;

/// One allowlist entry: a function symbol excused from the HIGH-`SKIP` gate,
/// legal ONLY for a non-recursion-participating boundary shape.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AllowlistEntry {
    /// Function symbol (e.g. `some_extern_boundary$direct`).
    pub(crate) symbol: String,
    /// Stable `ReasonCode` string (e.g. `NON_FLATTENABLE_PARAM`).
    #[allow(dead_code)]
    pub(crate) reason: String,
    /// Why this boundary shape cannot be a `musttail` edge (required rationale).
    #[allow(dead_code)]
    pub(crate) justification: String,
}

/// The parsed `tco-skip-allowlist.toml`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Allowlist {
    /// Schema version (must equal [`SCHEMA_VERSION`]).
    pub(crate) schema_version: u32,
    /// Allowlisted boundary shapes.
    #[serde(default)]
    pub(crate) skip: Vec<AllowlistEntry>,
}

impl Allowlist {
    /// An empty allowlist (used when no file is found — the gate then requires
    /// zero HIGH-risk `SKIP`).
    pub(crate) fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            skip: Vec::new(),
        }
    }

    /// Load + validate the allowlist from `path`.
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read allowlist {}: {e}", path.display()))?;
        let al: Allowlist = toml::from_str(&text)
            .map_err(|e| format!("cannot parse allowlist {}: {e}", path.display()))?;
        if al.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "allowlist {} has schema_version {} (expected {SCHEMA_VERSION})",
                path.display(),
                al.schema_version
            ));
        }
        Ok(al)
    }

    /// Whether a coverage-row source name is covered by any allowlist entry.
    /// Matches the bare name or a `$direct`/`$direct_mt` symbol form. Linear scan
    /// (the allowlist is tiny and usually empty) — no set, so no IR-determinism
    /// concern.
    fn covers(&self, name: &str) -> bool {
        self.skip.iter().any(|e| {
            e.symbol == name
                || e.symbol == format!("{name}$direct")
                || e.symbol == format!("{name}$direct_mt")
        })
    }
}

/// Walk up from `start` looking for `tools/tco-skip-allowlist.toml`; returns the
/// first hit (nearest ancestor), mirroring code-health's config discovery.
pub(crate) fn find_allowlist(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(ALLOWLIST_REL_PATH);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// The result of running the gate over an inventory.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct GateOutcome {
    /// HIGH-risk `SKIP` functions not covered by the allowlist.
    pub(crate) un_allowlisted: Vec<String>,
    /// Allowlist entries that ARE reported as a (self-recursive) HIGH-risk `SKIP`
    /// — recursion-participating, therefore rejected (R8).
    pub(crate) recursion_participating: Vec<String>,
}

impl GateOutcome {
    /// The gate passes iff there is nothing un-allowlisted AND no allowlist entry
    /// participates in recursion.
    pub(crate) fn passed(&self) -> bool {
        self.un_allowlisted.is_empty() && self.recursion_participating.is_empty()
    }
}

/// Run the gate over aggregated coverage rows + a loaded allowlist.
///
/// Every reported HIGH-risk `SKIP` is self-recursive (the inventory ranks only
/// self-recursive functions), so an allowlisted `SKIP` is recursion-participating
/// and is **rejected** rather than excused (R8).
pub(crate) fn run_gate(rows: &[FunctionCoverage], allowlist: &Allowlist) -> GateOutcome {
    let mut outcome = GateOutcome::default();
    for row in rows {
        if row.risk != Risk::High || row.decision != Decision::Skip {
            continue;
        }
        // Coverage rows are keyed by source name; allowlist entries by symbol.
        if allowlist.covers(&row.name) {
            // Self-recursive inventory ⇒ recursion-participating ⇒ reject (R8).
            outcome.recursion_participating.push(row.name.clone());
        } else {
            outcome.un_allowlisted.push(row.name.clone());
        }
    }
    outcome
}

/// Render the gate outcome to stderr and return whether it passed. Kept separate
/// from [`run_gate`] so the decision logic is pure + unit-testable.
pub(crate) fn report_gate(outcome: &GateOutcome) -> bool {
    if outcome.passed() {
        eprintln!(
            "✓ tco-coverage gate: 0 un-allowlisted HIGH-risk SKIP, 0 recursion-participating SKIP"
        );
        return true;
    }
    if !outcome.un_allowlisted.is_empty() {
        eprintln!(
            "✗ tco-coverage gate: {} un-allowlisted HIGH-risk SKIP:",
            outcome.un_allowlisted.len()
        );
        for name in &outcome.un_allowlisted {
            eprintln!("    {name}  (add indirect-param lowering, or — only if a non-recursive boundary — allowlist it)");
        }
    }
    if !outcome.recursion_participating.is_empty() {
        eprintln!(
            "✗ tco-coverage gate: {} allowlist entr{} participate in internal recursion (illegal, R8):",
            outcome.recursion_participating.len(),
            if outcome.recursion_participating.len() == 1 { "y" } else { "ies" }
        );
        for name in &outcome.recursion_participating {
            eprintln!("    {name}  (a recursion-participating SKIP is a hard failure even if allowlisted)");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_codegen::ReasonCode;

    fn row(name: &str, decision: Decision, risk: Risk) -> FunctionCoverage {
        FunctionCoverage {
            name: name.to_string(),
            decision,
            risk,
            reasons: if decision == Decision::Skip {
                vec![ReasonCode::NonFlattenableParam]
            } else {
                vec![]
            },
            driver: String::new(),
            skip_sites: usize::from(decision == Decision::Skip),
            emit_sites: usize::from(decision != Decision::Skip),
        }
    }

    fn allowlist_with(symbols: &[&str]) -> Allowlist {
        Allowlist {
            schema_version: 1,
            skip: symbols
                .iter()
                .map(|s| AllowlistEntry {
                    symbol: (*s).to_string(),
                    reason: "NON_FLATTENABLE_PARAM".into(),
                    justification: "test".into(),
                })
                .collect(),
        }
    }

    #[test]
    fn zero_high_skip_passes() {
        let rows = vec![
            row("spin", Decision::Decompose, Risk::Low),
            row("f", Decision::Emit, Risk::Low),
        ];
        let outcome = run_gate(&rows, &Allowlist::empty());
        assert!(outcome.passed(), "no HIGH SKIP → gate passes");
    }

    #[test]
    fn un_allowlisted_high_skip_fails() {
        let rows = vec![row("boom", Decision::Skip, Risk::High)];
        let outcome = run_gate(&rows, &Allowlist::empty());
        assert!(!outcome.passed());
        assert_eq!(outcome.un_allowlisted, vec!["boom".to_string()]);
        assert!(outcome.recursion_participating.is_empty());
    }

    #[test]
    fn allowlisted_self_recursive_skip_is_rejected_not_excused() {
        // A self-recursive SKIP that IS allowlisted is recursion-participating →
        // rejected (R8): the allowlist cannot excuse it.
        let rows = vec![row("boom", Decision::Skip, Risk::High)];
        let outcome = run_gate(&rows, &allowlist_with(&["boom"]));
        assert!(
            !outcome.passed(),
            "recursion-participating allowlist entry must fail"
        );
        assert!(outcome.un_allowlisted.is_empty());
        assert_eq!(outcome.recursion_participating, vec!["boom".to_string()]);
    }

    #[test]
    fn allowlist_matches_direct_symbol_forms() {
        // Allowlist keyed by `$direct` symbol still matches the source name.
        let rows = vec![row("boom", Decision::Skip, Risk::High)];
        let outcome = run_gate(&rows, &allowlist_with(&["boom$direct"]));
        assert_eq!(outcome.recursion_participating, vec!["boom".to_string()]);
    }

    #[test]
    fn inert_allowlist_entry_not_in_inventory_is_harmless() {
        // An allowlist entry that is not a reported SKIP neither fails nor excuses.
        let rows = vec![row("spin", Decision::Decompose, Risk::Low)];
        let outcome = run_gate(&rows, &allowlist_with(&["some_boundary$direct"]));
        assert!(outcome.passed());
    }

    #[test]
    fn med_and_low_skip_do_not_trip_the_gate() {
        // Only HIGH-risk SKIP is gated.
        let rows = vec![row("m", Decision::Skip, Risk::Med)];
        assert!(run_gate(&rows, &Allowlist::empty()).passed());
    }

    #[test]
    fn a_non_self_tail_edge_does_not_trip_the_gate() {
        // ADR 5.8.26a. A SKIP_NON_SELF row is a tail call INTO a function, not
        // a statement that the function grows its own stack — the gate must
        // ignore it even at HIGH risk. `build_coverage` already excludes these
        // from the inventory; this pins the second line of defence, so that
        // routing one into the gate by mistake cannot turn `make check-tco-gate`
        // red over the 1,286 benign edges the compiler emits.
        let rows = vec![row("callee", Decision::SkipNonSelf, Risk::High)];
        let outcome = run_gate(&rows, &Allowlist::empty());
        assert!(outcome.passed(), "{outcome:?}");
        assert!(outcome.un_allowlisted.is_empty());
        assert!(outcome.recursion_participating.is_empty());
    }
}
