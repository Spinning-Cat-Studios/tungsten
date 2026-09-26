//! `tungsten doctor check ir wrapper-self-calls` — ADR 23.7.26c D3.
//!
//! A monomorphized generic instance gets two symbols: the closure-returning
//! **wrapper** (`<W>`, allocates an environment per application step) and the
//! uncurried **`<W>$direct`** entry. A saturated first-order self-call must
//! ride `$direct`; if it re-enters the wrapper, each recursion step
//! heap-allocates an environment — on the self-compiled self-check's ctor-dedup hot loop
//! that integrated to ~36 GB (ADR 23.7.26c). This check audits emitted `.ll`
//! for exactly that regression: a depot instance whose own `$direct` /
//! `$direct_mt` / wrapper body `call`s its own wrapper `@W(`.
//!
//! **The call must actually yield a closure (ADR 28.7.26e §2.3).** Not every
//! instance *has* a closure wrapper: an arity-1 instance is emitted as a single
//! saturated symbol `@W` that returns its result aggregate and allocates no
//! environment, and its self-recursion is a plain direct call — correct, and
//! nothing to fix. The audit's first run over the self-hosted compiler's own IR
//! reported exactly one finding, and it was that shape:
//! `call { i32, [16 x i8] } @…_list_last_I_6String(ptr null, ptr %snd)` inside a
//! `define { i32, [16 x i8] }` with no `$direct` twin anywhere in the corpus.
//! The discriminator is the **return type**: re-entering a wrapper yields the
//! closure pair `{ ptr, ptr }`. So a self-call is flagged only when its callee
//! is exactly `@W` *and* the call returns that pair.
//!
//! **Precise, not broad.** It flags only a *self*-correlated wrapper call
//! (the block being defined is `W`, `W$direct`, or `W$direct_mt`). A
//! legitimate higher-order use — some *other* function building a closure over
//! an instance and calling its wrapper — is not flagged. Documented reach: it
//! keys on the `$direct`/wrapper define name, so an arity-1 instance recursing
//! only through an anonymous `____mono_lambda_N` body is not correlated (the
//! `compile/tests/depot_instance_callconv.rs` unit tests cover that shape on
//! controlled fixtures); and an instance whose *result* is itself a function
//! would return `{ ptr, ptr }` from a saturated entry, which this rule cannot
//! tell from a wrapper re-entry. Measured on the self-hosted compiler's corpus
//! (2,049 `.ll` files): zero findings.
//!
//! Run `tungsten compile --emit-llvm` first, then point this at the output dir.

use std::path::Path;
use std::process::ExitCode;

use super::corpus::{reject_non_directory, scan_ll_corpus, AuditVerdict, ReachCounts};
use super::textparse::parse_call;

#[cfg(test)]
mod tests;

/// The `$direct_mt` decomposed-entry suffix (mirrors
/// `tungsten_codegen::codegen::exec::direct_calls::decompose::DIRECT_MT_SUFFIX`).
const DIRECT_MT_SUFFIX: &str = "$direct_mt";
/// The `$direct` uncurried-entry suffix.
const DIRECT_SUFFIX: &str = "$direct";
/// The mangling marker present in every monomorphized-instance symbol.
const INSTANCE_MARKER: &str = "_I_";
/// What a closure-returning wrapper returns: `{ fn_ptr, env_ptr }`.
const CLOSURE_PAIR: &str = "{ ptr, ptr }";

/// A depot instance whose own body calls its closure-returning wrapper.
#[derive(Debug, PartialEq, Eq)]
pub struct WrapperSelfCallFinding {
    /// The `.ll` file the finding is in.
    pub file: String,
    /// The `define`d symbol whose body makes the offending call (`W`,
    /// `W$direct`, or `W$direct_mt`).
    pub define_symbol: String,
    /// The wrapper symbol `W` being re-entered.
    pub wrapper_symbol: String,
    /// 1-indexed line number of the offending `call`.
    pub line_num: usize,
    /// The trimmed offending line.
    pub line: String,
}

/// Per-module scan result: findings plus the parser's reach.
///
/// `candidates` counts correlated instance bodies entered and `tracked` the
/// call lines examined inside them. Counting *findings* here would make
/// `--strict` fail precisely when the compiler is correct, since a healthy
/// corpus has no self-calls at all — the reach measure has to be a quantity
/// that is non-zero on healthy IR (ADR 28.7.26e D4).
#[derive(Debug, Default)]
pub struct WrapperSelfCallSummary {
    pub findings: Vec<WrapperSelfCallFinding>,
    pub bodies: ReachCounts,
}

/// Scan a directory of `.ll` files for depot-instance wrapper self-calls.
pub fn cmd_check_wrapper_self_calls(dir: &Path, strict: bool) -> ExitCode {
    audit_directory(dir, strict).exit()
}

/// Scan + report, returning the verdict. The testable half of
/// [`cmd_check_wrapper_self_calls`], which is only this plus `.exit()`.
pub(crate) fn audit_directory(dir: &Path, strict: bool) -> AuditVerdict {
    if let Some(rejected) = reject_non_directory(dir) {
        return rejected;
    }

    let mut findings: Vec<WrapperSelfCallFinding> = Vec::new();
    let mut bodies = ReachCounts::default();
    let scan = scan_ll_corpus(dir, |path, text| {
        let module = scan_ll_content(&path.display().to_string(), text);
        bodies.add(module.bodies);
        findings.extend(module.findings);
    });
    scan.charge_unreadable(&mut bodies);

    println!(
        "summary: files={} instance_bodies={} calls_examined={} findings={}",
        scan.files,
        bodies.candidates,
        bodies.tracked,
        findings.len()
    );

    let verdict = AuditVerdict::classify(&scan, bodies.is_vacuous(), findings.len(), strict);
    match verdict {
        AuditVerdict::Violations(n) => {
            println!(
                "⚠ {n} depot-instance wrapper self-call(s) found \
                 (per-step env allocation — ADR 23.7.26c):\n"
            );
            for f in &findings {
                println!(
                    "  {}:{}: {} re-enters its wrapper @{}\n      {}",
                    f.file, f.line_num, f.define_symbol, f.wrapper_symbol, f.line
                );
            }
            println!(
                "\nHint: a saturated generic self-call must target `@<W>$direct`, \
                 not the wrapper `@<W>`.\n\
                 See `try_compile_direct_call` / `resolve_saturated_mono_callee`."
            );
        }
        AuditVerdict::Clean => println!(
            "✓ No depot-instance wrapper self-calls in {} ({} .ll file(s))",
            dir.display(),
            scan.files
        ),
        _ => {}
    }
    verdict.report_vacuity(
        bodies.is_vacuous(),
        "depot-instance bodies were entered but no call line was examined",
    );
    verdict
}

/// Scan one `.ll` file's content for wrapper self-calls.
///
/// Tracks the enclosing `define` block and, when that block is a depot
/// instance's own `$direct` / `$direct_mt` / wrapper body, flags any call to
/// its bare wrapper `@W` that yields a closure pair.
pub fn scan_ll_content(filename: &str, content: &str) -> WrapperSelfCallSummary {
    let mut summary = WrapperSelfCallSummary::default();
    // (define symbol F, correlated wrapper base W) for the current block.
    let mut current: Option<(String, String)> = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();

        if let Some(defined) = defined_symbol(trimmed) {
            current = wrapper_base(&defined).map(|w| (defined.clone(), w.to_string()));
            if current.is_some() {
                summary.bodies.note_candidate();
            }
            continue;
        }
        if trimmed == "}" {
            current = None;
            continue;
        }

        let Some((define_symbol, wrapper)) = &current else {
            continue;
        };
        let Some(call) = parse_call(line) else {
            continue;
        };
        summary.bodies.note_tracked();
        if !is_wrapper_reentry(&call, wrapper) {
            continue;
        }
        summary.findings.push(WrapperSelfCallFinding {
            file: filename.to_string(),
            define_symbol: define_symbol.clone(),
            wrapper_symbol: wrapper.clone(),
            line_num: i + 1,
            line: line.trim().to_string(),
        });
    }

    summary
}

/// Extract the symbol from a `define ... @NAME(` / `define ... @"NAME"(` line.
fn defined_symbol(line: &str) -> Option<String> {
    let rest = line.strip_prefix("define ")?;
    let after_at = &rest[rest.find('@')? + 1..];
    if let Some(quoted) = after_at.strip_prefix('"') {
        let end = quoted.find('"')?;
        Some(quoted[..end].to_string())
    } else {
        let end = after_at.find('(').unwrap_or(after_at.len());
        Some(after_at[..end].to_string())
    }
}

/// The wrapper base `W` this define correlates to, if it is a depot instance's
/// own entry (`W`, `W$direct`, or `W$direct_mt`) — else `None`.
fn wrapper_base(defined: &str) -> Option<&str> {
    let base = defined
        .strip_suffix(DIRECT_MT_SUFFIX)
        .or_else(|| defined.strip_suffix(DIRECT_SUFFIX))
        .unwrap_or(defined);
    if base.contains(INSTANCE_MARKER) {
        Some(base)
    } else {
        None
    }
}

/// True if the call targets the wrapper `@W` **and gets a closure back**.
///
/// Both halves are load-bearing. The callee comes from the call's callee
/// position, so an occurrence of `@W` among the arguments is not a re-entry;
/// and the closure-pair return type is what separates re-entering a wrapper
/// from an arity-1 instance calling its one saturated symbol (§2.3).
fn is_wrapper_reentry(call: &super::textparse::CallSite<'_>, wrapper: &str) -> bool {
    call.callee.trim_start_matches('@') == wrapper && call.return_type == CLOSURE_PAIR
}
