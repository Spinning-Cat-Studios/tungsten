//! `tungsten doctor check ir null-calls` — detect null function pointer calls in LLVM IR.
//!
//! Scans `.ll` files for call instructions whose **callee is the `null`
//! literal**, which indicates unresolved monomorphization: a mono instance was
//! expected but the function pointer was never filled in. This is the heuristic
//! that caught 282 null calls in ADR 10.5.26c.
//!
//! **Callee position, not substring (ADR 28.7.26e D2).** The original predicate
//! asked whether `null(` appeared anywhere after the `call` keyword. Every call
//! to a function whose *name* ends in `null` therefore matched: pointed at the
//! self-hosted compiler's own emitted IR for the first time, the audit reported
//! 15 findings and all 15 were `@cstring_is_null(`. The comment two lines above
//! the bug had promised the exclusion ("excluding @-prefixed symbols") since the
//! audit was written; nothing had ever run it against real IR to notice the
//! implementation never did it. The callee now comes from
//! [`callee_of`](super::textparse::callee_of).
//!
//! Text-based (like `check ir declares`/`indirect-buffers`): run
//! `tungsten compile --emit-llvm` first, then point this at the output dir.

use std::path::Path;
use std::process::ExitCode;

use super::corpus::{reject_non_directory, scan_ll_corpus, AuditVerdict, ReachCounts};
use super::textparse::callee_of;

#[cfg(test)]
mod tests;

/// A single finding: a line in a `.ll` file whose callee is `null`.
#[derive(Debug)]
pub struct NullCallFinding {
    pub file: String,
    pub line_num: usize,
    pub line: String,
}

/// Per-module scan result: findings plus the parser's reach.
///
/// `candidates` counts call instructions seen and `tracked` the ones whose
/// callee position resolved, so a change to emitted call syntax that blinds the
/// parser shows up as vacuity rather than as a clean bill of health.
#[derive(Debug, Default)]
pub struct NullCallSummary {
    pub findings: Vec<NullCallFinding>,
    pub calls: ReachCounts,
}

/// Scan a directory of `.ll` files for null function pointer calls.
pub fn cmd_check_null_calls(dir: &Path, strict: bool) -> ExitCode {
    audit_directory(dir, strict).exit()
}

/// Scan + report, returning the verdict. The testable half of
/// [`cmd_check_null_calls`], which is only this plus `.exit()`.
pub(crate) fn audit_directory(dir: &Path, strict: bool) -> AuditVerdict {
    if let Some(rejected) = reject_non_directory(dir) {
        return rejected;
    }

    let mut findings: Vec<NullCallFinding> = Vec::new();
    let mut calls = ReachCounts::default();
    let scan = scan_ll_corpus(dir, |path, text| {
        let module = scan_ll_content(&path.display().to_string(), text);
        calls.add(module.calls);
        findings.extend(module.findings);
    });
    scan.charge_unreadable(&mut calls);

    println!(
        "summary: files={} calls={} callees_resolved={} findings={}",
        scan.files,
        calls.candidates,
        calls.tracked,
        findings.len()
    );

    let verdict = AuditVerdict::classify(&scan, calls.is_vacuous(), findings.len(), strict);
    match verdict {
        AuditVerdict::Violations(n) => {
            println!("⚠ {n} null function pointer call(s) found:\n");
            for f in &findings {
                println!("  {}:{}: {}", f.file, f.line_num, f.line.trim());
            }
            println!(
                "\nHint: null calls indicate missed monomorphization.\n\
                 Check `tungsten info codegen mono` for the expected mono instances."
            );
        }
        AuditVerdict::Clean => println!(
            "✓ No null function pointer calls found in {} ({} .ll file(s))",
            dir.display(),
            scan.files
        ),
        _ => {}
    }
    verdict.report_vacuity(
        calls.is_vacuous(),
        "call instructions were seen but no callee position resolved",
    );
    verdict
}

/// Scan a single `.ll` file's content for null function pointer calls.
///
/// A finding is a call whose callee token is exactly the `null` literal —
/// `call i64 null(ptr null)`. A named callee is never one, however its name
/// ends.
pub fn scan_ll_content(filename: &str, content: &str) -> NullCallSummary {
    let mut summary = NullCallSummary::default();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !is_call_instruction(trimmed) {
            continue;
        }
        summary.calls.note_candidate();
        let Some(callee) = callee_of(trimmed) else {
            continue;
        };
        summary.calls.note_tracked();
        if callee == "null" {
            summary.findings.push(NullCallFinding {
                file: filename.to_string(),
                line_num: i + 1,
                line: line.to_string(),
            });
        }
    }

    summary
}

/// Whether the line carries a `call`/`invoke` keyword at all — the candidate
/// test, kept separate from resolving the callee so "saw a call but could not
/// parse its callee" is measurable rather than silent.
fn is_call_instruction(trimmed: &str) -> bool {
    !is_global_definition(trimmed) && (trimmed.contains("call ") || trimmed.contains("invoke "))
}

/// A module-scope global definition (`@str_lit.7 = … c" = call \00"`) is data,
/// not an instruction.
///
/// The self-hosted compiler emits LLVM IR *as string literals*, so its own IR
/// carries globals whose text reads like a call — the same defect class that
/// produced all five of `declares`' false findings (ADR 28.7.26e). Two such
/// globals in the corpus were counted as calls whose callee would not resolve;
/// a literal that happened to spell a *null* callee would have been a finding.
fn is_global_definition(trimmed: &str) -> bool {
    trimmed.starts_with('@') && trimmed.contains(" = ")
}
