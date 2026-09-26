//! Table + JSON rendering for `tco-coverage` (ADR 1.7.26b §2.1).

use tungsten_codegen::{Decision, MusttailDecision, ReasonCode};

use super::risk::{FunctionCoverage, Risk};

/// Rendering options derived from CLI flags.
pub(crate) struct RenderOpts {
    /// Only show rows at or above HIGH risk.
    pub(crate) risk_high_only: bool,
    /// Include EMIT/DECOMPOSE (LOW-risk) rows.
    pub(crate) show_emit: bool,
}

/// Human-readable one-line reason summary derived from the stable enum set.
/// `struct param + struct return` collapses to `struct param+ret`.
pub(crate) fn reason_summary(reasons: &[ReasonCode]) -> String {
    if reasons.is_empty() {
        return "—".to_string();
    }
    let has_ret = reasons.contains(&ReasonCode::StructReturn);
    let has_param = reasons
        .iter()
        .any(|r| matches!(r, ReasonCode::StructParam | ReasonCode::NonFlattenableParam));
    let non_flat = reasons.contains(&ReasonCode::NonFlattenableParam);
    if has_ret && has_param {
        let base = "struct param+ret".to_string();
        return if non_flat {
            format!("{base} (non-flattenable)")
        } else {
            base
        };
    }
    if has_ret {
        return "struct return".to_string();
    }
    if non_flat {
        return "struct param (non-flattenable)".to_string();
    }
    if has_param {
        return "struct param".to_string();
    }
    reasons
        .iter()
        .map(|r| r.human())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Filter rows per options.
fn visible_rows<'a>(rows: &'a [FunctionCoverage], opts: &RenderOpts) -> Vec<&'a FunctionCoverage> {
    rows.iter()
        .filter(|r| {
            if opts.risk_high_only {
                return r.risk == Risk::High;
            }
            opts.show_emit || r.decision == Decision::Skip
        })
        .collect()
}

/// Render the ranked coverage table (ADR 1.7.26b §2.1).
///
/// `non_self` is the `(sites, callees)` pair from
/// [`super::risk::non_self_tail_sites`] — the tail edges this table
/// deliberately does not rank (ADR 5.8.26a). It is printed rather than dropped
/// so a reader can see that "0 HIGH fns" is a statement about self-recursive
/// functions only, and how large the unranked population is.
pub(crate) fn render_table(
    rows: &[FunctionCoverage],
    opts: &RenderOpts,
    non_self: (usize, usize),
) -> String {
    let bar = "─".repeat(76);
    let mut out = String::new();
    out.push_str("MUSTTAIL COVERAGE — self-recursive functions\n");
    out.push_str(&bar);
    out.push('\n');
    out.push_str(&format!(
        "{:<5} {:<20} {:<9} {:<24} {}\n",
        "RISK", "FUNCTION", "DECISION", "REASON", "RECURSES OVER"
    ));
    out.push_str(&bar);
    out.push('\n');

    let shown = visible_rows(rows, opts);
    for r in &shown {
        out.push_str(&format!(
            "{:<5} {:<20} {:<9} {:<24} {}\n",
            r.risk.code(),
            truncate(&r.name, 20),
            r.decision.code(),
            truncate(&reason_summary(&r.reasons), 24),
            r.driver,
        ));
    }

    out.push_str(&bar);
    out.push('\n');
    out.push_str(&totals_line(rows));
    out.push('\n');
    out.push_str(&non_self_line(non_self));
    out.push('\n');
    out
}

/// Summary totals line: SKIP sites · EMIT sites · HIGH functions.
fn totals_line(rows: &[FunctionCoverage]) -> String {
    let skip_sites: usize = rows.iter().map(|r| r.skip_sites).sum();
    let emit_sites: usize = rows.iter().map(|r| r.emit_sites).sum();
    let high_fns = rows.iter().filter(|r| r.risk == Risk::High).count();
    format!(
        "{skip_sites} SKIP sites · {emit_sites} EMIT sites · {high_fns} HIGH fns   \
         (HIGH = SKIP + collection-driven, O(N) stack)"
    )
}

/// The unranked-population line (ADR 5.8.26a): tail edges to a *different*
/// function, which this table does not rank and `--gate` does not judge.
fn non_self_line((sites, callees): (usize, usize)) -> String {
    format!(
        "{sites} SKIP_NON_SELF sites over {callees} callee(s) — tail edges to another \
         function, not ranked here and not gated (mutual-tail musttail is \
         unimplemented; classifying them needs the recursion call graph)"
    )
}

/// Truncate a string to `width` chars, appending `…` when clipped.
fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(width.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Render the raw per-call-site records unaggregated (`--by-site`).
pub(crate) fn render_by_site(decisions: &[MusttailDecision]) -> String {
    let bar = "─".repeat(76);
    let mut out = String::new();
    out.push_str("MUSTTAIL DECISIONS — per tail-call site\n");
    out.push_str(&bar);
    out.push('\n');
    out.push_str(&format!(
        "{:<9} {:<28} {}\n",
        "DECISION", "FUNCTION", "REASON"
    ));
    out.push_str(&bar);
    out.push('\n');
    for d in decisions {
        out.push_str(&format!(
            "{:<9} {:<28} {}\n",
            d.decision.code(),
            truncate(d.function.as_str(), 28),
            reason_summary(&d.reasons),
        ));
    }
    out.push_str(&bar);
    out.push('\n');
    out
}

/// Render the machine-readable JSON (ADR 1.7.26b §2.1 contract).
pub(crate) fn render_json(rows: &[FunctionCoverage], decisions: &[MusttailDecision]) -> String {
    let functions: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name,
                "decision": r.decision.code(),
                "risk": r.risk.code(),
                "reasons": r.reasons.iter().map(|x| x.code()).collect::<Vec<_>>(),
                "driver": r.driver,
                "sites": { "skip": r.skip_sites, "emit": r.emit_sites },
            })
        })
        .collect();

    let sites: Vec<serde_json::Value> = decisions
        .iter()
        .map(|d| {
            serde_json::json!({
                "function": d.base_name(),
                "symbol": d.function,
                "decision": d.decision.code(),
                "reasons": d.reasons.iter().map(|x| x.code()).collect::<Vec<_>>(),
                "lowered_sig": d.lowered_sig,
            })
        })
        .collect();

    let skip: usize = rows.iter().map(|r| r.skip_sites).sum();
    let emit: usize = rows.iter().map(|r| r.emit_sites).sum();
    let high = rows.iter().filter(|r| r.risk == Risk::High).count();
    let (non_self_sites, non_self_callees) = super::risk::non_self_tail_sites(decisions);

    let output = serde_json::json!({
        "functions": functions,
        "sites": sites,
        "totals": {
            "skip": skip,
            "emit": emit,
            "high": high,
            // ADR 5.8.26a: the unranked population — tail edges to another
            // function, present in `sites` but never in `functions`.
            "skip_non_self": non_self_sites,
            "skip_non_self_callees": non_self_callees,
        },
    });
    serde_json::to_string_pretty(&output).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_summary_collapses_param_and_ret() {
        assert_eq!(
            reason_summary(&[ReasonCode::StructReturn, ReasonCode::StructParam]),
            "struct param+ret"
        );
        assert_eq!(
            reason_summary(&[ReasonCode::StructReturn, ReasonCode::NonFlattenableParam]),
            "struct param+ret (non-flattenable)"
        );
    }

    #[test]
    fn reason_summary_singletons() {
        assert_eq!(reason_summary(&[ReasonCode::StructReturn]), "struct return");
        assert_eq!(reason_summary(&[ReasonCode::StructParam]), "struct param");
        assert_eq!(
            reason_summary(&[ReasonCode::NonFlattenableParam]),
            "struct param (non-flattenable)"
        );
        assert_eq!(reason_summary(&[]), "—");
    }
}
