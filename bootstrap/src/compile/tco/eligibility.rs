//! `info codegen musttail-eligibility <fn> <file>` — per-function drill-down
//! (ADR 1.7.26b §2.3).
//!
//! Reports, for one function, whether its lowered signature passes the musttail
//! gate and — if not — which return/param aggregate blocks it and whether that
//! aggregate is flattenable. When `<fn>` resolves to multiple monomorph
//! specializations, one block is printed per lowered signature.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_codegen::{Decision, MusttailDecision, ReasonCode};

use super::collect::collect_musttail_decisions;

/// Entry point for `tungsten info codegen musttail-eligibility <fn> <file>`.
pub(crate) fn cmd_musttail_eligibility(
    fn_name: &str,
    file: &PathBuf,
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

    let matches: Vec<&MusttailDecision> = run
        .decisions
        .iter()
        .filter(|d| d.base_name() == fn_name)
        .collect();

    if matches.is_empty() {
        eprintln!(
            "no self-recursive musttail decision found for '{fn_name}'. \
             It may be non-recursive, not tail-recursive, or not present in this file."
        );
        return ExitCode::FAILURE;
    }

    print!("{}", render_blocks(fn_name, &matches));
    ExitCode::SUCCESS
}

/// Render one block per lowered signature (specialization).
fn render_blocks(fn_name: &str, matches: &[&MusttailDecision]) -> String {
    let multi = matches.len() > 1;
    let mut out = String::new();
    for (i, d) in matches.iter().enumerate() {
        let label = if multi {
            format!("{fn_name}  (specialization {} of {})", i + 1, matches.len())
        } else {
            fn_name.to_string()
        };
        out.push_str(&format!("{label}  —  musttail: {}\n", d.decision.code()));
        out.push_str(&format!("  lowered signature: {}\n", d.lowered_sig));
        if d.blockers.is_empty() {
            out.push_str("  → eligible: no ABI blockers\n");
        } else {
            out.push_str("  blocker(s):\n");
            for b in &d.blockers {
                out.push_str(&format!(
                    "    • {:<8} {} → {}\n",
                    b.position.human(),
                    b.lowered_type,
                    b.reason.human(),
                ));
            }
            out.push_str(&format!("  decomposition: {}\n", decomposition_note(d)));
        }
        out.push('\n');
    }
    out
}

/// Human note on whether the function is decomposition-eligible.
fn decomposition_note(d: &MusttailDecision) -> &'static str {
    match d.decision {
        Decision::Decompose => "eligible (decomposed $direct_mt entry emits musttail)",
        _ if d.reasons.contains(&ReasonCode::NonFlattenableParam) => {
            "ineligible (nested struct/array fields, or >8 fields)"
        }
        _ if d.reasons.contains(&ReasonCode::StructReturn) => {
            "return blocks musttail (needs sret ABI — see ADR 1.7.26a)"
        }
        _ => "n/a",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_codegen::{Blocker, BlockerPosition};

    fn decision(name: &str, dec: Decision, blockers: Vec<Blocker>) -> MusttailDecision {
        let reasons = blockers.iter().map(|b| b.reason).collect();
        MusttailDecision {
            function: format!("{name}$direct"),
            decision: dec,
            reasons,
            blockers,
            lowered_sig: "{ i32, [56 x i8] }(ptr, {…}, ptr)".to_string(),
            param_abi: Vec::new(),
            sret: false,
            slot_attrs: Vec::new(),
        }
    }

    #[test]
    fn names_both_blockers_for_struct_param_and_return() {
        let blockers = vec![
            Blocker {
                position: BlockerPosition::Return,
                reason: ReasonCode::StructReturn,
                lowered_type: "{ i32, [8 x i8] }".to_string(),
            },
            Blocker {
                position: BlockerPosition::Param(1),
                reason: ReasonCode::NonFlattenableParam,
                lowered_type: "{ ptr, { ptr } }".to_string(),
            },
        ];
        let d = decision("collect_type_names", Decision::Skip, blockers);
        let out = render_blocks("collect_type_names", &[&d]);
        assert!(out.contains("musttail: SKIP"));
        assert!(out.contains("return"));
        assert!(out.contains("struct return"));
        assert!(out.contains("param 1"));
        assert!(out.contains("non-flattenable"));
        assert!(out.contains("ineligible"));
    }

    #[test]
    fn multi_specialization_prints_each_block() {
        let d1 = decision("f", Decision::Skip, vec![]);
        let d2 = decision("f", Decision::Emit, vec![]);
        let out = render_blocks("f", &[&d1, &d2]);
        assert!(out.contains("specialization 1 of 2"));
        assert!(out.contains("specialization 2 of 2"));
    }
}
