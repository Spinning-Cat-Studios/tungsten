//! Per-function aggregation for musttail coverage (ADR 1.7.26b §2.1).
//!
//! Folds per-tail-call-site [`MusttailDecision`]s into function-level rows,
//! ranked by the O(N)-stack risk [`driver`] derives from each function's
//! parameter types.
//!
//! The population this aggregates is **self-recursive functions**. Non-self tail
//! edges ([`Decision::SkipNonSelf`]) are counted by [`non_self_tail_sites`] and
//! deliberately excluded from the rows — see `build_coverage` and ADR 5.8.26a.

mod driver;

use std::collections::BTreeMap;

use tungsten_codegen::{Decision, MusttailDecision, ReasonCode};
use tungsten_core::types::Type;

use driver::{classify_risk, collect_param_types, driver_display};

/// O(N)-stack risk rank for a self-recursive function (ADR 1.7.26b §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Risk {
    /// SKIP with a collection/unbounded driver ⇒ O(input) stack.
    High,
    /// SKIP with a structurally bounded finite driver.
    Med,
    /// SKIP but the driver could not be classified.
    Unknown,
    /// EMIT / DECOMPOSE — musttail (or the decomposed entry) achieves constant stack.
    Low,
}

impl Risk {
    /// Machine-stable code string for `--json` / snapshots.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Risk::High => "HIGH",
            Risk::Med => "MED",
            Risk::Unknown => "UNKNOWN",
            Risk::Low => "LOW",
        }
    }
}

/// Function-level coverage row aggregated from all of a function's decision sites.
#[derive(Debug, Clone)]
pub(crate) struct FunctionCoverage {
    /// Source-level function name.
    pub(crate) name: String,
    /// Aggregated outcome (best-decision-wins on the constant-stack axis).
    pub(crate) decision: Decision,
    /// Risk rank.
    pub(crate) risk: Risk,
    /// Union of ABI reasons across the function's SKIP sites.
    pub(crate) reasons: Vec<ReasonCode>,
    /// Display of the recursion driver type (e.g. `List<Item>`).
    pub(crate) driver: String,
    /// Number of SKIP sites.
    pub(crate) skip_sites: usize,
    /// Number of constant-stack (EMIT/DECOMPOSE) sites.
    pub(crate) emit_sites: usize,
}

/// Aggregate raw decisions into ranked, per-function coverage rows.
///
/// Rows are sorted HIGH→LOW then by name so table/JSON output is deterministic.
///
/// [`Decision::SkipNonSelf`] sites are **excluded** (ADR 5.8.26a): this
/// inventory ranks self-recursive functions by O(N)-stack risk, and a non-self
/// tail edge is a fact about a *call*, not about whether its callee recurses.
/// Including them would key rows by callee — 637 of them on
/// `src/compiler/main.tg`, mostly non-recursive — and rank any that takes a
/// `List`/`Nat`/`String` as HIGH, which `run_gate` would then fail on. They stay
/// in the raw decision stream, where `--by-site` and `--json` count them; see
/// [`non_self_tail_sites`].
pub(crate) fn build_coverage(
    decisions: &[MusttailDecision],
    fn_types: &BTreeMap<String, Type>,
) -> Vec<FunctionCoverage> {
    // Group decisions by base (source-level) function name.
    let mut by_fn: BTreeMap<&str, Vec<&MusttailDecision>> = BTreeMap::new();
    for d in decisions.iter().filter(|d| d.decision.is_self_recursive()) {
        by_fn.entry(d.base_name()).or_default().push(d);
    }

    let mut rows: Vec<FunctionCoverage> = by_fn
        .into_iter()
        .map(|(name, sites)| aggregate(name, &sites, fn_types))
        .collect();

    rows.sort_by(|a, b| {
        risk_order(a.risk)
            .cmp(&risk_order(b.risk))
            .then(a.name.cmp(&b.name))
    });
    rows
}

/// How many recorded sites are non-self tail edges, and how many distinct
/// callees they reach (ADR 5.8.26a).
///
/// These are the sites [`build_coverage`] excludes. Reporting the pair is what
/// makes the mutual-tail residual *countable* rather than invisible: it is the
/// population a future call-graph join would have to classify, and a
/// `SKIP_NON_SELF` edge landing on a function `doctor audit-recursion` reports
/// as recursion-participating is the falsifiable trigger that re-activates
/// ADR 1.7.26e Non-Goal #3.
pub(crate) fn non_self_tail_sites(decisions: &[MusttailDecision]) -> (usize, usize) {
    let edges: Vec<&MusttailDecision> = decisions
        .iter()
        .filter(|d| !d.decision.is_self_recursive())
        .collect();
    let callees: std::collections::BTreeSet<&str> = edges.iter().map(|d| d.base_name()).collect();
    (edges.len(), callees.len())
}

/// Sort key: HIGH first, LOW last.
fn risk_order(r: Risk) -> u8 {
    match r {
        Risk::High => 0,
        Risk::Med => 1,
        Risk::Unknown => 2,
        Risk::Low => 3,
    }
}

/// Aggregate one function's sites into a coverage row.
fn aggregate(
    name: &str,
    sites: &[&MusttailDecision],
    fn_types: &BTreeMap<String, Type>,
) -> FunctionCoverage {
    let emit_sites = sites
        .iter()
        .filter(|d| d.decision.is_constant_stack())
        .count();
    let skip_sites = sites.len() - emit_sites;

    // Best-decision-wins on the constant-stack axis (a decomposed entry rescues
    // a skipping base entry). Prefer EMIT, then DECOMPOSE, else SKIP.
    let decision = if sites.iter().any(|d| d.decision == Decision::Emit) {
        Decision::Emit
    } else if sites.iter().any(|d| d.decision == Decision::Decompose) {
        Decision::Decompose
    } else {
        Decision::Skip
    };

    // Union of reasons across SKIP sites, in a stable order.
    let mut reasons: Vec<ReasonCode> = Vec::new();
    for d in sites {
        for r in &d.reasons {
            if !reasons.contains(r) {
                reasons.push(*r);
            }
        }
    }

    let params = fn_types
        .get(name)
        .map(collect_param_types)
        .unwrap_or_default();
    let driver = driver_display(&params);
    let risk = classify_risk(decision, &params);

    FunctionCoverage {
        name: name.to_string(),
        decision,
        risk,
        reasons,
        driver,
        skip_sites,
        emit_sites,
    }
}

#[cfg(test)]
mod tests {
    use super::driver::arrow;
    use super::*;

    fn skip_decision(name: &str, reasons: Vec<ReasonCode>) -> MusttailDecision {
        MusttailDecision {
            function: format!("{name}$direct"),
            decision: Decision::Skip,
            reasons,
            blockers: Vec::new(),
            lowered_sig: String::new(),
            param_abi: Vec::new(),
            sret: false,
            slot_attrs: Vec::new(),
        }
    }

    fn emit_decision(name: &str) -> MusttailDecision {
        MusttailDecision {
            function: format!("{name}$direct"),
            decision: Decision::Emit,
            reasons: Vec::new(),
            blockers: Vec::new(),
            lowered_sig: String::new(),
            param_abi: Vec::new(),
            sret: false,
            slot_attrs: Vec::new(),
        }
    }

    #[test]
    fn aggregate_unions_reasons_and_counts_sites() {
        let decisions = vec![
            skip_decision("f", vec![ReasonCode::StructReturn]),
            skip_decision("f", vec![ReasonCode::StructParam]),
        ];
        let mut types = BTreeMap::new();
        types.insert(
            "f".to_string(),
            arrow(vec![Type::App("List".into(), vec![Type::Nat])], Type::Nat),
        );
        let rows = build_coverage(&decisions, &types);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].skip_sites, 2);
        assert_eq!(rows[0].emit_sites, 0);
        assert_eq!(rows[0].risk, Risk::High);
        assert_eq!(rows[0].reasons.len(), 2);
    }

    #[test]
    fn aggregate_decompose_rescues_skip_to_low() {
        let decisions = vec![
            skip_decision("g", vec![ReasonCode::StructParam]),
            MusttailDecision {
                function: "g$direct_mt".into(),
                decision: Decision::Decompose,
                reasons: vec![],
                blockers: vec![],
                lowered_sig: String::new(),
                param_abi: Vec::new(),
                sret: false,
                slot_attrs: Vec::new(),
            },
        ];
        let mut types = BTreeMap::new();
        types.insert("g".to_string(), arrow(vec![Type::String], Type::Nat));
        let rows = build_coverage(&decisions, &types);
        assert_eq!(rows[0].decision, Decision::Decompose);
        assert_eq!(rows[0].risk, Risk::Low);
    }

    fn non_self_decision(callee: &str) -> MusttailDecision {
        MusttailDecision {
            function: format!("{callee}$direct"),
            decision: Decision::SkipNonSelf,
            reasons: vec![ReasonCode::NonFlattenableParam],
            blockers: Vec::new(),
            lowered_sig: String::new(),
            param_abi: Vec::new(),
            sret: false,
            slot_attrs: Vec::new(),
        }
    }

    #[test]
    fn non_self_tail_edges_are_kept_out_of_the_self_recursive_inventory() {
        // ADR 5.8.26a. `append_all` is a plain non-recursive helper that other
        // functions tail-call; it takes a List, so ranking it as a SKIP would
        // make it HIGH and fail the gate. It must produce no row at all.
        let decisions = vec![
            non_self_decision("append_all"),
            non_self_decision("append_all"),
            skip_decision("risky", vec![ReasonCode::StructReturn]),
        ];
        let mut types = BTreeMap::new();
        let list_of_nat = arrow(vec![Type::App("List".into(), vec![Type::Nat])], Type::Nat);
        types.insert("append_all".to_string(), list_of_nat.clone());
        types.insert("risky".to_string(), list_of_nat);

        let rows = build_coverage(&decisions, &types);
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["risky"],
            "only the self-recursive function may be ranked"
        );
        // The edges are not lost — they are counted, which is what makes the
        // mutual-tail residual visible rather than merely absent.
        assert_eq!(non_self_tail_sites(&decisions), (2, 1));
    }

    #[test]
    fn a_function_that_is_both_a_tail_target_and_self_recursive_keeps_its_row() {
        // The exclusion is per-SITE, not per-function: `walk` recurses AND is
        // tail-called from elsewhere. Dropping the whole function would hide a
        // genuine HIGH-risk SKIP behind an unrelated call site.
        let decisions = vec![
            non_self_decision("walk"),
            skip_decision("walk", vec![ReasonCode::NonFlattenableParam]),
        ];
        let mut types = BTreeMap::new();
        types.insert(
            "walk".to_string(),
            arrow(vec![Type::App("List".into(), vec![Type::Nat])], Type::Nat),
        );
        let rows = build_coverage(&decisions, &types);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "walk");
        assert_eq!(rows[0].risk, Risk::High);
        assert_eq!(
            rows[0].skip_sites, 1,
            "the non-self edge is not a SKIP site"
        );
        assert_eq!(non_self_tail_sites(&decisions), (1, 1));
    }

    #[test]
    fn non_self_tail_sites_counts_sites_and_distinct_callees() {
        let decisions = vec![
            non_self_decision("a"),
            non_self_decision("b"),
            non_self_decision("a"),
            emit_decision("self_rec"),
        ];
        assert_eq!(non_self_tail_sites(&decisions), (3, 2));
        assert_eq!(non_self_tail_sites(&[]), (0, 0));
    }

    #[test]
    fn rows_sorted_high_before_low() {
        let decisions = vec![
            emit_decision("safe"),
            skip_decision("risky", vec![ReasonCode::StructReturn]),
        ];
        let mut types = BTreeMap::new();
        types.insert("safe".to_string(), arrow(vec![Type::Nat], Type::Nat));
        types.insert(
            "risky".to_string(),
            arrow(vec![Type::App("List".into(), vec![Type::Nat])], Type::Nat),
        );
        let rows = build_coverage(&decisions, &types);
        assert_eq!(rows[0].name, "risky");
        assert_eq!(rows[0].risk, Risk::High);
        assert_eq!(rows[1].name, "safe");
        assert_eq!(rows[1].risk, Risk::Low);
    }
}
