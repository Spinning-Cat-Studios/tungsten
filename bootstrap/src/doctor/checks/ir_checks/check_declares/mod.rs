//! Declaration hygiene check for per-unit LLVM IR (ADR 10.5.26b §2.2).
//!
//! Scans `.ll` files and validates that every direct `call @symbol` target
//! has a matching `declare` or `define` in the same file.
//!
//! **Function bodies only (ADR 28.7.26e).** Pointed at the self-hosted
//! compiler's own emitted IR for the first time, this reported five missing
//! declarations — and all five were *string literals*. The self-hosted compiler
//! emits LLVM IR as text, so its own IR is full of module-scope constants like
//! `@str_lit.3 = … c"  %result = call i64 @tungsten_main$direct(ptr null)\00"`.
//! A module-scope global is not an instruction, so call targets are now
//! collected inside `define … { … }` bodies only. Residual reach: an IR-text
//! string literal appearing *inside* a function body would still match; the
//! compiler emits its literals at module scope.

mod scanner;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::ExitCode;

use super::corpus::{reject_non_directory, scan_ll_corpus, AuditVerdict, ReachCounts};

/// Run the check-declares scan on all `.ll` files in a directory.
pub fn cmd_check_declares(dir: &Path, strict: bool) -> ExitCode {
    audit_directory(dir, strict).exit()
}

/// Scan + report, returning the verdict. The testable half of
/// [`cmd_check_declares`], which is only this plus `.exit()`.
pub(crate) fn audit_directory(dir: &Path, strict: bool) -> AuditVerdict {
    if let Some(rejected) = reject_non_directory(dir) {
        return rejected;
    }

    // `candidates` counts call targets seen and `tracked` the ones resolved
    // against a declare/define, so a syntax change that blinds the extractor
    // reads as drift rather than as universal hygiene (ADR 28.7.26e D4).
    let mut targets = ReachCounts::default();
    let mut missing: Vec<(std::path::PathBuf, scanner::MissingDeclaration)> = Vec::new();
    let scan = scan_ll_corpus(dir, |path, ir| {
        let module = scanner::scan_declarations(ir);
        targets.add(module.targets);
        for m in module.missing {
            missing.push((path.to_path_buf(), m));
        }
    });
    scan.charge_unreadable(&mut targets);

    println!(
        "summary: files={} call_targets={} resolved={} missing={}",
        scan.files,
        targets.candidates,
        targets.tracked,
        missing.len()
    );

    let verdict = AuditVerdict::classify(&scan, targets.is_vacuous(), missing.len(), strict);
    match verdict {
        AuditVerdict::Violations(n) => {
            for (file, m) in &missing {
                let rel = file.strip_prefix(dir).unwrap_or(file);
                eprintln!(
                    "  ✗ {}: call @{} has no declare or define (line {})",
                    rel.display(),
                    m.symbol,
                    m.line_number,
                );
            }
            eprintln!("\n{n} missing declaration(s).");
        }
        AuditVerdict::Clean => println!(
            "✓ All call targets have matching declare/define statements ({} .ll file(s)).",
            scan.files
        ),
        _ => {}
    }
    verdict.report_vacuity(
        targets.is_vacuous(),
        "call targets were seen but none resolved against a declare/define",
    );
    verdict
}
