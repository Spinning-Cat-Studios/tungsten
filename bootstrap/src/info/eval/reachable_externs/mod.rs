//! `tungsten info eval reachable-externs <def> <file>` — which externs a
//! definition's call path reaches, and which of those the evaluator cannot run.
//!
//! Cost 3: elaborates the project, then walks Core. No codegen.
//!
//! ## The question this answers, and why the neighbouring tools do not
//!
//! The evaluator executes only allowlisted externs; every other `ExternCall`
//! goes **silently `Stuck`**, so an assertion whose operand reaches one never
//! runs and `tungsten test` reports `ok`. Two commands already circle this and
//! neither answers it for a *specific definition*:
//!
//! * `tungsten info eval externs` lists the allowlist — the global set, with no
//!   notion of your code.
//! * `tungsten doctor check extern-coverage <file>` reports **declaration**
//!   scope: every `extern "C" fn` the module tree declares. Measured on
//!   `src/compiler/test_test_verb.tg`, it reports **138 of 150 declared externs
//!   unexecutable** for a file whose every assertion provably executes. It
//!   reads identically for a sound file and a vacuous one, which is exactly the
//!   caveat `.claude/CLAUDE.md` records.
//!
//! The gap between them cost ADR 7.8.26a a mid-implementation redesign: §2.3
//! prescribed `parent_directory` + `path_join`, and only reading
//! `EXECUTABLE_EXTERNS` by hand revealed that neither is executable, so the
//! test written against them would have been the very vacuity the ADR existed
//! to prevent. This command answers that in one cost-3 invocation.
//!
//! ## How an extern appears in Core
//!
//! An `extern "C" fn tg_foo(..)` declaration elaborates to a **definition of
//! its own**, whose body is `λ… . ExternCall("__c_tg_foo", …)`; call sites
//! reference it as `Global("tg_foo")`. So one uniform walk over the Core graph
//! finds both hops, and the `__c_` prefix is stripped here exactly as
//! `step_extern_call_env` strips it before consulting the registry — matching
//! the raw symbol would report every extern unexecutable.
//!
//! ## Why several roots, and why a budget (ADR 3.9.26c)
//!
//! The elaboration is ~99% of the invocation and the walk is free: measured on
//! `src/compiler/main.tg` with a release binary, a 6-definition walk cost 31.0 s
//! and a 524-definition walk 30.2 s. So `--defs` fans the walk out over several
//! roots from **one** elaboration — the shape 19.8.26c established as normal,
//! since the measurement that means anything is taken on a definition's call
//! sites, before and after. And `--max-visited` bounds each walk, because an
//! unbounded one that returns nothing cannot be told from a wedged one; hitting
//! it is a *distinct verdict* naming what went unread, never a shorter list.

mod render;
mod report;
mod walk;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::Term;

use tungsten_bootstrap::driver;

use report::TestReferences;

pub use walk::DEFAULT_MAX_VISITED;

/// Everything `tungsten info eval reachable-externs` needs, as one value —
/// the `TraceOptions` idiom, because the set form and the budget take the
/// argument list past the repo's five-parameter threshold.
pub struct ReachableExternsOptions {
    /// The root named positionally: the single-definition spelling, unchanged.
    pub name: String,
    /// Additional roots from `--defs`, comma-separated and/or repeated.
    pub defs: Vec<String>,
    /// The source file (or project entry file).
    pub file: PathBuf,
    /// Definitions one walk may enter before it reports a partial answer.
    pub max_visited: usize,
    /// Emit JSON — an object for the single form, an array for a set.
    pub json: bool,
}

/// The roots to walk, in request order, with duplicates dropped.
///
/// PURE, and split out because it decides the two things the output shape hangs
/// on: what order the sections come in, and whether this invocation is the
/// single form (`--json` object) or a set (`--json` array).
///
/// `--defs` values are split on commas so `--defs a,b` and `--defs a --defs b`
/// mean the same thing; blanks are dropped rather than walked as `""`.
pub(crate) fn resolve_roots(name: &str, defs: &[String]) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    let mut push = |candidate: &str| {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() && !roots.iter().any(|r| r == trimmed) {
            roots.push(trimmed.to_string());
        }
    };
    push(name);
    for value in defs {
        for part in value.split(',') {
            push(part);
        }
    }
    roots
}

/// What the command prints, once the walks are done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReportShape {
    /// The single-definition spelling: one section, and in `--json` the bare
    /// object every existing consumer already parses (D3).
    Single,
    /// The set form: sections in request order, and in `--json` an outer array.
    Set,
    /// No root resolved, so the errors on stderr are the whole output — never
    /// an empty array or an empty section, which would read as a clean answer
    /// about definitions nothing looked at.
    Nothing,
}

/// Which shape this invocation prints.
///
/// PURE, and split from the driver because it is the one decision the driver
/// makes that a unit test can reach: the elaboration around it cannot be
/// injected, so left inline this reads as covered while being asserted by
/// nothing (`testing-patterns.md` § A pure-function seam).
///
/// The set form is chosen by the REQUEST, not by how many roots resolved, so a
/// consumer that asked for an array is never handed a bare object because one
/// name was misspelled.
pub(crate) fn report_shape(roots_requested: usize, reports_resolved: usize) -> ReportShape {
    if reports_resolved == 0 {
        ReportShape::Nothing
    } else if roots_requested > 1 {
        ReportShape::Set
    } else {
        ReportShape::Single
    }
}

/// Entry point for `tungsten info eval reachable-externs`.
///
/// One elaboration, then one walk per root against the same globals and the
/// same `test_*` index.
pub fn cmd_info_eval_reachable_externs(
    opts: &ReachableExternsOptions,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let project = match driver::elaborate_project(&opts.file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let globals: BTreeMap<String, Term> = project
        .defs
        .iter()
        .map(|d| (d.name.clone(), d.term.term.clone()))
        .collect();
    let tests = TestReferences::index(&globals);

    let roots = resolve_roots(&opts.name, &opts.defs);
    let mut reports = Vec::new();
    let mut missing = Vec::new();
    for root in &roots {
        match walk::analyze(&globals, root, opts.max_visited, &tests) {
            Some(report) => reports.push(report),
            None => missing.push(root.clone()),
        }
    }

    // `Single` and `Set` both imply at least one report, so the indexing below
    // is total.
    match (report_shape(roots.len(), reports.len()), opts.json) {
        (ReportShape::Nothing, _) => {}
        (ReportShape::Set, true) => println!("{}", render::render_json_array(&reports)),
        (ReportShape::Set, false) => {
            print!("{}", render::render_human_sections(&reports, &opts.file));
        }
        (ReportShape::Single, true) => println!("{}", render::render_json(&reports[0])),
        (ReportShape::Single, false) => print!("{}", render::render_human(&reports[0], &opts.file)),
    }

    for name in &missing {
        eprintln!(
            "error: no definition named '{name}' in {}",
            opts.file.display()
        );
    }
    if missing.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
