//! `tungsten doctor check comparable <type> <file>` — can the structural
//! comparator handle this type, and if not, exactly where does it break?
//!
//! Cost 3 (elaboration only; no codegen, no evaluation of user code).
//!
//! ## The failure this makes visible
//!
//! Before ADR 1.8.26b, `compare<T>` at a type whose comparator could not be
//! synthesized did not report an error — it left the call **Stuck**. The
//! enclosing `assert_eq` never executed, the test-failure flag was never set,
//! and `tungsten test` reported the test **`ok`**. A suite of only-passing
//! assertions could not distinguish "compared and equal" from "never compared
//! at all". ADR 29.6.26f's close-out found a 23-test suite of which roughly
//! nine asserted nothing.
//!
//! 1.8.26b's D3 gate turned that silence into a hard failure, so a broken
//! comparison now fails the run rather than passing it. This check keeps its
//! value for the cheaper order: it answers the same question **before** the
//! tests are written, in about a second, and it names *where* the comparator
//! breaks rather than only *that* it does.
//!
//! ## What it reports
//!
//! Exactly what the evaluator enforces. [`analyse_comparability`] runs
//! [`gate::classify`] — the same function the `__cmp<T>` callback calls — so
//! the diagnostic cannot disagree with what a run will do. The classes are
//! separated because their fixes differ:
//!
//! 1. **Opaque leaf** — a closure, arena handle or other §2.2-noncomparable
//!    leaf makes the enclosing type noncomparable *by policy*, reported with
//!    the path to the first offender (`$.env: EvalEnv is opaque`). A design
//!    decision, not a bug: normalize it away or compare a projection.
//! 2. **Incomplete closure** — a synthesized body calls a `compare_*` symbol
//!    the closure never defines. Since 1.8.26b this is what an *unresolvable
//!    μ-cluster member* looks like: the nested-μ encoding does not carry the
//!    other members' bodies, so a member with no stored encoding cannot be
//!    synthesized (`comparator::context`).
//! 3. **Unsettled synthesis** — the closure walk hit its bound with work still
//!    queued, so the type's comparator does not converge.
//!
//! ### Two arms were removed at 1.8.26b, and why
//!
//! The pre-1.8.26b check also carried a **wide-constructor** arm (≥3 payload
//! fields) and an over-approximating **μ-binder-count** arm. Both are gone
//! because both defects are fixed, and a check that reports a fixed defect
//! trains its readers to ignore it:
//!
//! - *Wide constructors* flagged the left-nested-type/right-nested-value
//!   mismatch. D1 made the type encoder right-nested, so `Product(Product(A,
//!   B), C)` is now the legitimate encoding of a **2**-field constructor whose
//!   first field is a tuple — indistinguishable from a 3-field one by shape
//!   alone. The arm could only have become a false positive.
//! - *μ-binder count* flagged the population D2 lived in without proving each
//!   member failed. D2's fix (one `Unfold` per chain, members resolved through
//!   provenance) makes multi-binder clusters compare correctly, and the exact
//!   class-2 arm catches the residual case — a member the project cannot
//!   resolve — by name.
//!
//! Exit codes: 0 comparable, 1 findings, 2 bad input (no such type, or the
//! file does not elaborate). An inability to look is never a finding.

use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::eval::ComparatorFailureKind;
use tungsten_core::types::Type;

use crate::comparator::gate;
use crate::comparator::ComparatorTypes;
use crate::driver;
use crate::driver::ProjectOutput;

mod render;

#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tests;

pub(crate) use render::render_report;

/// Everything the check learned about one type. Kept as a value (rather than
/// printed as it goes) so the analysis is unit-testable without a filesystem,
/// an elaborated project, or captured stdout.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ComparabilityReport {
    /// Why the gate rejected this type, or `None` when it accepted it.
    pub failure: Option<ComparatorFailureKind>,
    /// How many comparators the closure emitted. Non-zero on an accepted type
    /// — a "proof" arm that examined nothing would report every type clean,
    /// which is the vacuity failure this check exists to prevent.
    pub closure_size: usize,
}

impl ComparabilityReport {
    /// Whether the comparator can be trusted at this type.
    pub fn is_comparable(&self) -> bool {
        self.failure.is_none()
    }

    pub fn exit(&self) -> ExitCode {
        if self.is_comparable() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        }
    }
}

/// What the CLI's two positionals and `--all` resolve to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// Check one named type in `file`.
    One { name: String, file: PathBuf },
    /// Check every type `file` declares.
    Every { file: PathBuf },
}

/// Resolve the CLI arguments into a target, or explain what is wrong.
///
/// Both positionals are `Option` because clap refuses an optional positional
/// ahead of a required one, and `<TYPE> <FILE>` was the shipped order — so the
/// arity check lives here instead. Keeping it a pure function means the
/// ambiguous forms are unit-testable without spawning the binary, and the error
/// text is asserted rather than hoped for.
pub(crate) fn resolve_target(
    type_name: Option<String>,
    file: Option<PathBuf>,
    all: bool,
) -> Result<Target, String> {
    match (all, type_name, file) {
        // `--all <file>`: the sole positional lands in the first slot.
        (true, Some(file), None) => Ok(Target::Every {
            file: PathBuf::from(file),
        }),
        (true, None, Some(file)) => Ok(Target::Every { file }),
        (true, Some(_), Some(_)) => Err(
            "`--all` checks every type, so it takes only a file — drop the type name".to_string(),
        ),
        (true, None, None) => Err("`--all` needs a file to check".to_string()),
        (false, Some(name), Some(file)) => Ok(Target::One { name, file }),
        // One positional and no `--all` is the easy mistake: it looks like a
        // file, so say what to add rather than repeating clap's arity message.
        (false, Some(_), None) => {
            Err("expected a type name AND a file — did you mean `--all <file>`?".to_string())
        }
        (false, None, _) => Err("expected a type name and a file".to_string()),
    }
}

/// CLI entry: resolve the arguments, then check one type or all of them.
pub fn run(type_name: Option<String>, file: Option<PathBuf>, all: bool, verbose: bool) -> ExitCode {
    match resolve_target(type_name, file, all) {
        Ok(Target::One { name, file }) => {
            cmd_check_comparable(Some(&name), &file, false, verbose, 20)
        }
        Ok(Target::Every { file }) => cmd_check_comparable(None, &file, true, verbose, 20),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// Entry point for `tungsten doctor check comparable [<type>|--all] <file>`.
///
/// Elaboration dominates: on the compiler's own module graph it is ~88 s while
/// the analysis itself is milliseconds, so `--all` exists to pay that once
/// rather than once per type (measured over the 1.8.26b close-out ladder, where
/// four single-type runs cost ~6 minutes of which ~5:52 was re-elaborating the
/// same graph).
pub fn cmd_check_comparable(
    name: Option<&str>,
    file: &PathBuf,
    all: bool,
    verbose: bool,
    max_errors: usize,
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let types = ComparatorTypes::new(
        project.record_types.clone(),
        &project.encoded_types,
        &project.type_provenance,
        project.adt_types.clone(),
        &project.mutual_recursion_groups,
    );

    if all {
        return report_every_type(&project, &types, file);
    }

    let name = name.expect("clap requires a type name unless --all");
    let Some(encoded) = resolve_encoded_type(name, &project) else {
        eprintln!(
            "error: no stored encoding for type `{name}` in {}",
            file.display()
        );
        eprintln!(
            "hint:  `tungsten info type types {}` lists the types this file declares.",
            file.display()
        );
        return ExitCode::from(2);
    };

    let report = analyse_comparability(encoded, &types);
    print!("{}", render_report(name, &report));
    report.exit()
}

/// Check every type the project stores an encoding for, in one elaboration.
///
/// Sorted by name so two runs over the same file produce identical output —
/// `encoded_types` is a `HashMap`, and a summary whose order changes between
/// invocations cannot be diffed.
///
/// An empty corpus is **bad input (exit 2), not a clean bill**: a file that
/// declares no types is a question this check cannot answer, and reporting
/// "all comparable" over nothing is the vacuous-green failure the whole check
/// exists to prevent.
fn report_every_type(project: &ProjectOutput, types: &ComparatorTypes, file: &PathBuf) -> ExitCode {
    let mut names: Vec<&String> = project.encoded_types.keys().collect();
    names.sort();

    if names.is_empty() {
        eprintln!(
            "error: {} declares no types with a stored encoding",
            file.display()
        );
        return ExitCode::from(2);
    }

    let reports: Vec<(&str, ComparabilityReport)> = names
        .into_iter()
        .map(|name| {
            let report = analyse_comparability(&project.encoded_types[name], types);
            (name.as_str(), report)
        })
        .collect();

    print!("{}", render::render_summary(&reports));
    if reports.iter().all(|(_, r)| r.is_comparable()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Look up a named type's stored (Encoding Finalization) encoding.
fn resolve_encoded_type<'a>(name: &str, project: &'a ProjectOutput) -> Option<&'a Type> {
    project.encoded_types.get(name)
}

/// The whole analysis, as a pure function over the two things it needs.
///
/// Delegates to [`gate::classify`] rather than re-deriving a verdict. That is
/// the load-bearing property: this command exists to tell a test author what a
/// run will do, so it must not be able to reach a different conclusion from the
/// run (the `run_chain`/`explain` discipline the guardrail hooks use).
pub(crate) fn analyse_comparability(ty: &Type, types: &ComparatorTypes) -> ComparabilityReport {
    match gate::classify(ty, types) {
        Ok(gated) => ComparabilityReport {
            failure: None,
            closure_size: gated.defs.len(),
        },
        Err(failure) => ComparabilityReport {
            failure: Some(failure.kind),
            closure_size: 0,
        },
    }
}
