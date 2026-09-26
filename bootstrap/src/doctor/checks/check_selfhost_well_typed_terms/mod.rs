//! `tungsten doctor check selfhost well-typed-terms` — does every eliminator
//! the self-hosted compiler emits stand over something it can eliminate?
//!
//! The sibling of `doctor check selfhost closed-terms`, and a **separate**
//! check rather than an extension of it (ADR 3.9.26h D1): the two answer
//! different questions, and a caller needs to know which one failed.
//! `closed-terms` asks whether every name in an elaborated body is bound. It
//! answers `0 of 2298` over `src/compiler/main.tg` — and **both** defects that
//! surfaced while closing ADR 21.8.26a pass it:
//!
//! ```text
//! constructor application   App(App(λx:(A × B). fold (inr x), 9), N2)
//! tuple projection          let b : B = fst (snd __tup)   where snd __tup : B
//! ```
//!
//! The first applies a unary lambda's *result* to a second argument; the second
//! takes `fst` of something no recorded type says is a pair. Both are closed.
//! Neither is visible to any other signal the repo has: codegen builds CIR from
//! the pattern and never reads the Core term, and the type checker resolves
//! through an environment that is still in scope when it runs.
//!
//! ## What it asks, and what it does not
//!
//! Shape agreement, not typing (D2): whether an eliminator's operand has the
//! right **former** — `Fst`/`Snd` over a `Product`, `App` over an arrow, `Case`
//! over a `Sum`, `Unfold` over a `Mu`. Where the elaborator recorded no type
//! there is no finding.
//!
//! ## Why this shells out
//!
//! The question is about the **self-hosted** compiler's output, which this
//! binary cannot produce. So the check runs `tungsten1 check <file>
//! --check-well-typed` and reads its census, the same way
//! `diff bootstrap-selfhost-check` runs both compilers.
//!
//! The parsing is a pure function over the captured text
//! ([`parse_census`]); only [`probe`] touches a subprocess. That split is what
//! makes every verdict — including the three "cannot answer" ones — assertable
//! without a self-compile.

mod verdict;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::{Command, ExitCode};

/// Both checks need the same two inputs and name the same two absences, so the
/// guard is reused rather than re-written — a second copy is a second place for
/// the order of the two conditions to be flipped without any test noticing.
pub use super::check_selfhost_closed_terms::{preflight, MissingInput};
pub use verdict::{
    baseline_for, exit_code_for, outcome_for, parse_census, Census, IllShapedDefinition, Outcome,
    ProbeVerdict, BASELINE_CORPUS, MAIN_TG_BASELINE,
};

/// Run the check and report.
pub fn cmd_check_selfhost_well_typed_terms(
    file: &Path,
    selfhost_binary: &Path,
    verbose: bool,
) -> ExitCode {
    if let Err(missing) = preflight(file, selfhost_binary) {
        match missing {
            MissingInput::SourceFile => {
                eprintln!("error: source file not found: {}", file.display());
            }
            MissingInput::SelfhostBinary => {
                eprintln!(
                    "error: self-host binary not found: {}",
                    selfhost_binary.display()
                );
                eprintln!("  hint: build one with `make devcontainer-self-compile-dev`");
            }
        }
        return ExitCode::FAILURE;
    }

    let output = match probe(file, selfhost_binary) {
        Ok(text) => text,
        Err(error) => {
            eprintln!(
                "error: failed to run {}: {error}",
                selfhost_binary.display()
            );
            eprintln!("  hint: ensure `ulimit -s 65536` before running tungsten1");
            return ExitCode::FAILURE;
        }
    };

    if verbose {
        println!("--- {} --check-well-typed ---", selfhost_binary.display());
        println!("{output}");
    }

    report(
        &outcome_for(&parse_census(&output), baseline_for(file)),
        file,
    )
}

/// Run the self-host binary's shape census and capture what it said.
///
/// Both streams are captured and concatenated: the census is written to
/// stderr, and a build that refuses the flag may explain itself on either.
fn probe(file: &Path, selfhost_binary: &Path) -> std::io::Result<String> {
    let output = Command::new(selfhost_binary)
        .arg("check")
        .arg(file)
        .arg("--check-well-typed")
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

/// Render an outcome and choose the exit code.
///
/// **Three of the four outcomes are non-zero, and two of those are "cannot
/// answer" rather than "found something".** A version of this check that exited
/// 0 when it had examined nothing would reproduce the very defect it exists to
/// catch.
///
/// The arms carry no conditions — `outcome_for` decided already — so there is
/// no second copy of the census logic here to drift from the first.
fn report(outcome: &Outcome<'_>, file: &Path) -> ExitCode {
    match outcome {
        Outcome::CannotAsk(reason) => report_cannot_ask(reason),
        Outcome::ExaminedNothing => {
            eprintln!(
                "❌ the self-host examined 0 definitions in {} — a clean verdict over \
                 an empty corpus proves nothing",
                file.display()
            );
        }
        Outcome::Clean { examined } => {
            println!(
                "✅ every eliminator stands over a former that admits it: {examined} \
                 definition(s) examined in {}, 0 with shape mismatches",
                file.display()
            );
        }
        Outcome::AtBaseline {
            examined,
            ill_shaped,
        } => {
            println!(
                "✅ no new shape mismatch: {} of {examined} definition(s) in {}, the \
                 recorded baseline",
                ill_shaped.len(),
                file.display()
            );
            list(ill_shaped);
        }
        Outcome::Regressed {
            examined,
            ill_shaped,
            baseline,
        } => report_regression(*examined, ill_shaped, *baseline, file),
        Outcome::BaselineStale { found, baseline } => {
            println!(
                "❌ {found} definition(s) with shape mismatches, {baseline} recorded — \
                 lower MAIN_TG_BASELINE to {found}"
            );
            println!();
            println!("The baseline is shrink-only on purpose: a gain nobody records is a");
            println!("gain the next change can give back for free.");
        }
    }
    ExitCode::from(exit_code_for(outcome))
}

/// One line per ill-shaped definition.
fn list(ill_shaped: &[IllShapedDefinition]) {
    println!();
    for entry in ill_shaped {
        println!("  {}: {}", entry.definition, entry.mismatches.join("; "));
    }
}

/// List what was found, and say why nothing else would have found it.
fn report_regression(
    examined: usize,
    ill_shaped: &[IllShapedDefinition],
    baseline: usize,
    file: &Path,
) {
    println!(
        "❌ {} of {examined} definition(s) in {} eliminate a term whose recorded type \
         refuses it, {baseline} allowed",
        ill_shaped.len(),
        file.display()
    );
    list(ill_shaped);
    println!();
    println!("An eliminator over the wrong former is closed and type-checks — the");
    println!("environment that resolved it is in scope when the check runs — and codegen");
    println!("never reads the Core term at all. What breaks is everything that DOES:");
    println!("`tungsten1 run` prints an unreduced term and says only \"not a value\".");
}

/// Why the self-host could not be asked, and what to do about it.
fn report_cannot_ask(reason: &ProbeVerdict) {
    match reason {
        ProbeVerdict::Stubbed => {
            eprintln!(
                "❌ the self-host binary has no diagnostic tools compiled in, so it \
                 examined nothing"
            );
            eprintln!(
                "   the production build stubs them out (ADR 18.4.26f); rebuild with \
                 `make devcontainer-self-compile-dev`"
            );
        }
        ProbeVerdict::NotExecutable => {
            eprintln!("❌ the self-host binary could not be executed on this machine");
            eprintln!(
                "   `tungsten1` is built for the devcontainer's platform — run this \
                 check there:"
            );
            eprintln!("   devcontainer exec --workspace-folder . <this command>");
        }
        ProbeVerdict::NoCensus => {
            eprintln!(
                "❌ the self-host binary printed no shape census — it is older than \
                 `--check-well-typed`, or the check did not reach elaboration"
            );
            eprintln!("   re-run with -v to see what it did print");
        }
        ProbeVerdict::Examined(_) => {
            eprintln!("ICE: an examined census is not a reason the check could not run");
        }
    }
}
