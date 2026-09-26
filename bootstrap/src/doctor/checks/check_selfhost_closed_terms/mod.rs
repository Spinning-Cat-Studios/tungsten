//! `tungsten doctor check selfhost closed-terms` — is every elaborated body
//! the self-hosted compiler produces **closed**?
//!
//! A free value variable in a Core term means the elaborator resolved a name
//! through its own environment and never emitted the binding. The body still
//! type-checks — that environment is in scope when the check runs — and then
//! nothing downstream can follow the value: the evaluator gets stuck, and any
//! analysis that reasons by following bindings (termination's descent, a size
//! analysis, a call-graph audit) silently concludes the wrong thing.
//!
//! ADR 19.8.26d found exactly that shape and could only find it by predicting
//! it from the lowering source and building a fixture pair to confirm. It had
//! been invisible for as long as it existed, because the *compiled* path
//! rebuilds the bindings from the pattern and never reads the Core term — so a
//! green self-compile says nothing about it.
//!
//! ## Why this shells out
//!
//! The question is about the **self-hosted** compiler's output, which this
//! binary cannot produce. So the check runs `tungsten1 check <file>
//! --check-free-vars` and reads its census, the same way
//! `diff bootstrap-selfhost-check` runs both compilers.
//!
//! The parsing is a pure function over the captured text
//! ([`parse_census`]); only [`probe`] touches a subprocess. That split is what
//! makes every verdict — including the two "cannot answer" ones — assertable
//! without a self-compile.

mod verdict;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::process::{Command, ExitCode};

pub use verdict::{
    exit_code_for, outcome_for, parse_census, preflight, Census, MissingInput, OpenDefinition,
    Outcome, ProbeVerdict,
};

/// Run the check and report.
pub fn cmd_check_selfhost_closed_terms(
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
        println!("--- {} --check-free-vars ---", selfhost_binary.display());
        println!("{output}");
    }

    report(&outcome_for(&parse_census(&output)), file)
}

/// Run the self-host binary's free-variable census and capture what it said.
///
/// Both streams are captured and concatenated: the census is written to
/// stderr, and a build that refuses the flag may explain itself on either.
fn probe(file: &Path, selfhost_binary: &Path) -> std::io::Result<String> {
    let output = Command::new(selfhost_binary)
        .arg("check")
        .arg(file)
        .arg("--check-free-vars")
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(text)
}

/// Render an outcome and choose the exit code.
///
/// **Three of the four outcomes are non-zero, and two of those are "cannot
/// answer" rather than "found something".** That is deliberate: this check
/// exists because a silent no-op looked like a clean bill of health, and a
/// version of it that exited 0 when it had examined nothing would reproduce
/// the very defect it was written to catch.
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
                "✅ every Core term is closed: {examined} definition(s) examined in {}, \
                 0 with free variables",
                file.display()
            );
        }
        Outcome::Findings { examined, open } => {
            println!(
                "❌ {} of {examined} definition(s) in {} elaborate to a term with free \
                 variable(s)",
                open.len(),
                file.display()
            );
            println!();
            for entry in *open {
                println!(
                    "  {}: {}",
                    entry.definition,
                    entry.free_variables.join(", ")
                );
            }
            println!();
            println!("A free variable means the elaborator resolved that name through its own");
            println!("environment and never emitted the binding. The body type-checks and the");
            println!("compiled path works, because codegen rebuilds the binding from the");
            println!("pattern — but the Core term is what every analysis reads (ADR 21.8.26a).");
        }
    }
    ExitCode::from(exit_code_for(outcome))
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
                "❌ the self-host binary printed no free-variable census — it is older \
                 than `--check-free-vars`, or the check did not reach elaboration"
            );
            eprintln!("   re-run with -v to see what it did print");
        }
        ProbeVerdict::Examined(_) => {
            eprintln!("ICE: an examined census is not a reason the check could not run");
        }
    }
}
