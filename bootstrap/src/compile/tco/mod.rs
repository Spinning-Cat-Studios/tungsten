//! `tco-coverage` + `musttail-eligibility` diagnostics (ADR 1.7.26b).
//!
//! These surface the *actual* codegen-time musttail decision — the
//! `check_musttail_abi_safety` gate — as a ranked, inspectable inventory,
//! turning silent O(N)-stack SKIPs into a visible risk table before they
//! overflow. All three surfaces (here + the `audit-recursion` bridge) consume
//! the structured [`tungsten_codegen::MusttailDecision`] records collected in
//! [`collect`].

use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub(crate) mod collect;
mod eligibility;
mod gate;
mod indirect_abi;
mod render;
mod risk;
#[cfg(test)]
mod tests;

pub(crate) use collect::{collect_musttail_decisions, MusttailRun};
pub(crate) use eligibility::cmd_musttail_eligibility;
pub(crate) use indirect_abi::cmd_indirect_abi;

use render::RenderOpts;

/// CLI options for `doctor check tco-coverage`.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TcoCoverageOpts {
    /// Emit machine-readable JSON instead of the table.
    pub(crate) json: bool,
    /// Filter to HIGH-risk rows only.
    pub(crate) risk_high: bool,
    /// List per-call-site records instead of aggregated function rows.
    pub(crate) by_site: bool,
    /// Also list EMIT/DECOMPOSE (LOW-risk) rows.
    pub(crate) emit: bool,
    /// Run as a deterministic CI gate (ADR 1.7.26e): exit non-zero on any
    /// un-allowlisted HIGH-risk `SKIP` or recursion-participating allowlist entry.
    pub(crate) gate: bool,
}

/// Codegen-consulting entry for `tungsten doctor audit-recursion <file>`
/// (ADR 1.7.26b §2.2). Runs codegen, builds per-function musttail verdicts, and
/// hands them to the source-level audit so over-optimistic `✓ musttail eligible`
/// verdicts are downgraded to the *actual* SKIP. Falls back to the source-level
/// estimate (labelled `codegen-failed`) if codegen errors.
pub(crate) fn cmd_audit_recursion_bridged(
    file: &PathBuf,
    source_only: bool,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    use tungsten_bootstrap::doctor::audit_recursion::{
        cmd_audit_recursion_with_verdicts, AnalysisMode, CodegenVerdict,
    };
    use tungsten_codegen::Decision;

    if source_only {
        return cmd_audit_recursion_with_verdicts(
            file,
            verbose,
            max_errors,
            AnalysisMode::SourceOnly,
            &std::collections::HashMap::new(),
        );
    }

    match collect_musttail_decisions(file, verbose, max_errors) {
        Ok(run) => {
            let rows = risk::build_coverage(&run.decisions, &run.fn_types);
            let verdicts = rows
                .iter()
                .map(|r| {
                    (
                        r.name.clone(),
                        CodegenVerdict {
                            skipped: r.decision == Decision::Skip,
                            reason: render::reason_summary(&r.reasons),
                        },
                    )
                })
                .collect();
            cmd_audit_recursion_with_verdicts(
                file,
                verbose,
                max_errors,
                AnalysisMode::Codegen,
                &verdicts,
            )
        }
        Err(e) => cmd_audit_recursion_with_verdicts(
            file,
            verbose,
            max_errors,
            AnalysisMode::CodegenFailed(e),
            &std::collections::HashMap::new(),
        ),
    }
}

/// Entry point for `tungsten doctor check tco-coverage <file>`.
pub(crate) fn cmd_check_tco_coverage(
    file: &PathBuf,
    opts: TcoCoverageOpts,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let run = match collect_musttail_decisions(file, verbose, max_errors) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let rows = risk::build_coverage(&run.decisions, &run.fn_types);

    if opts.gate {
        return run_coverage_gate(&rows);
    }

    if opts.json {
        println!("{}", render::render_json(&rows, &run.decisions));
        return ExitCode::SUCCESS;
    }

    if opts.by_site {
        print!("{}", render::render_by_site(&run.decisions));
        return ExitCode::SUCCESS;
    }

    let render_opts = RenderOpts {
        risk_high_only: opts.risk_high,
        show_emit: opts.emit,
    };
    let non_self = risk::non_self_tail_sites(&run.decisions);
    print!("{}", render::render_table(&rows, &render_opts, non_self));
    ExitCode::SUCCESS
}

/// Run the deterministic `--gate` check (ADR 1.7.26e §5): load the checked-in
/// `tools/tco-skip-allowlist.toml` (empty if absent) and fail on any
/// un-allowlisted HIGH-risk `SKIP` or recursion-participating allowlist entry.
fn run_coverage_gate(rows: &[risk::FunctionCoverage]) -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let allowlist = match load_gate_allowlist(&cwd) {
        Ok(al) => al,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = gate::run_gate(rows, &allowlist);
    if gate::report_gate(&outcome) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Load the checked-in gate allowlist, or an empty one when none is present.
fn load_gate_allowlist(cwd: &Path) -> Result<gate::Allowlist, String> {
    match gate::find_allowlist(cwd) {
        Some(path) => gate::Allowlist::load(&path),
        None => Ok(gate::Allowlist::empty()),
    }
}
