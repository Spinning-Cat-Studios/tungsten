//! Reading the self-host's term out of its output, and saying how the two
//! differ.
//!
//! Pure, for the same reason the closed-terms verdict is: the interesting
//! cases — a stubbed binary, a definition the self-host does not have, a
//! divergence in the middle of a long term — are all reachable here without a
//! self-compile.

use std::process::ExitCode;

/// What the self-host had to say about the definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfhostTerm {
    /// It rendered a term.
    Rendered(String),
    /// It has no diagnostic tools compiled in.
    Stubbed,
    /// It ran and reported which names matched, but not this one.
    NotFound,
    /// It never reported a dump at all — it predates `--dump-core-terms`, or
    /// the check did not reach elaboration.
    Unsupported,
    /// It could not be executed at all — most often a Linux `tungsten1`
    /// invoked from the macOS host rather than inside the devcontainer.
    NotExecutable,
    /// It matched the definition but could not read the term back.
    Unreadable,
}

/// How the two compilers compare on one definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreComparison {
    /// Byte-identical renderings.
    Agree(String),
    /// Different terms, with the index of the first differing byte.
    Diverge {
        bootstrap: String,
        selfhost: String,
        first_difference: usize,
    },
    /// The self-host could not be asked.
    Unanswerable(SelfhostTerm),
}

/// The marker a stubbed build prints when a diagnostic flag is requested.
const STUBBED_MARKER: &str = "[diagnostics] this binary has no diagnostic tools";

/// The prefix of a machine-readable term line.
const CORE_PREFIX: &str = "[core] ";

/// What a shell says when asked to run a binary built for another platform.
/// Distinguished because "rebuild it" and "run it in the container" are
/// different fixes and the wrong one wastes a build.
const NOT_EXECUTABLE_MARKERS: &[&str] = &[
    "cannot execute binary file",
    "Exec format error",
    "Permission denied",
];

/// The self-host's census line, which reports how many names matched.
const DUMP_CENSUS_PREFIX: &str = "[dump-core] ";

/// What `tg_diagnostic_core_term` yields when the handle will not read.
const UNREADABLE: &str = "<unreadable>";

/// Pull the term for `definition` out of the self-host's captured output.
///
/// Matching on the *name* rather than taking the first `[core]` line matters
/// because `--dump-core-terms '*'` is legal: a caller that passed a pattern
/// would otherwise silently compare against whichever definition came first.
#[must_use]
pub fn parse_core_line(output: &str, definition: &str) -> SelfhostTerm {
    if output.contains(STUBBED_MARKER) {
        return SelfhostTerm::Stubbed;
    }
    if NOT_EXECUTABLE_MARKERS
        .iter()
        .any(|marker| output.contains(marker))
    {
        return SelfhostTerm::NotExecutable;
    }
    let wanted = format!("{definition}\t");
    for line in output.lines() {
        let Some(rest) = line.trim_start().strip_prefix(CORE_PREFIX) else {
            continue;
        };
        if let Some(term) = rest.strip_prefix(&wanted) {
            if term.trim() == UNREADABLE {
                return SelfhostTerm::Unreadable;
            }
            return SelfhostTerm::Rendered(term.to_string());
        }
    }
    // A census saying zero matched is a *definite* "no such definition". No
    // census at all means the dump never ran — an older binary silently skips
    // an unknown flag and then reads its argument as the filename — and that
    // is a different fix, so it must not be reported as a missing definition.
    if output
        .lines()
        .any(|l| l.trim_start().starts_with(DUMP_CENSUS_PREFIX))
    {
        SelfhostTerm::NotFound
    } else {
        SelfhostTerm::Unsupported
    }
}

/// Compare this compiler's rendering against the self-host's.
#[must_use]
pub fn compare(bootstrap: &str, selfhost: &SelfhostTerm) -> CoreComparison {
    let SelfhostTerm::Rendered(theirs) = selfhost else {
        return CoreComparison::Unanswerable(selfhost.clone());
    };
    if bootstrap == theirs {
        return CoreComparison::Agree(bootstrap.to_string());
    }
    CoreComparison::Diverge {
        bootstrap: bootstrap.to_string(),
        selfhost: theirs.clone(),
        first_difference: first_difference(bootstrap, theirs),
    }
}

/// The byte index at which two differing strings first disagree.
///
/// Reported because these terms run to hundreds of characters and share a long
/// prefix: "they differ" is not actionable, "they differ at 41" points at the
/// construct.
pub(super) fn first_difference(left: &str, right: &str) -> usize {
    left.bytes()
        .zip(right.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| left.len().min(right.len()))
}

/// Which inputs the comparison cannot start without.
///
/// A value rather than an inline print, so a flipped guard is distinguishable
/// in a test: both absences exit the same way. Mirrors
/// `diff bootstrap-selfhost-check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingInput {
    SelfhostBinary,
}

/// The self-host binary must exist before it is spawned.
///
/// # Errors
/// Names the absent input. The source file is not checked here — the bootstrap
/// elaborates it first and reports its own error if it is missing.
pub fn preflight(selfhost_binary: &std::path::Path) -> Result<(), MissingInput> {
    if selfhost_binary.exists() {
        Ok(())
    } else {
        Err(MissingInput::SelfhostBinary)
    }
}

/// The exit code a comparison produces.
///
/// Split from [`report`] as a plain integer rather than left inside it as an
/// `ExitCode`: `ExitCode` implements no equality, so the mapping from verdict
/// to the number the shell sees would only be assertable by spawning the
/// binary. Three of the four codes are non-zero, and two of those mean
/// "could not answer" rather than "found something" — a distinction worth a
/// test of its own.
#[must_use]
pub fn exit_code_for(comparison: &CoreComparison) -> u8 {
    match comparison {
        CoreComparison::Agree(_) => 0,
        CoreComparison::Diverge { .. } => 1,
        CoreComparison::Unanswerable(_) => 2,
    }
}

/// Print the comparison and choose the exit code.
pub fn report(comparison: &CoreComparison) -> ExitCode {
    let code = exit_code_for(comparison);
    match comparison {
        CoreComparison::Agree(term) => {
            println!("✅ both compilers elaborate this definition to the same Core term");
            println!();
            println!("  {term}");
            ExitCode::from(code)
        }
        CoreComparison::Diverge {
            bootstrap,
            selfhost,
            first_difference,
        } => {
            println!("❌ the two compilers produce different Core terms");
            println!();
            println!("  bootstrap: {bootstrap}");
            println!("  self-host: {selfhost}");
            println!();
            println!("  first difference at byte {first_difference}:");
            println!(
                "    bootstrap: …{}",
                tail_from(bootstrap, *first_difference)
            );
            println!("    self-host: …{}", tail_from(selfhost, *first_difference));
            ExitCode::from(code)
        }
        CoreComparison::Unanswerable(reason) => {
            report_unanswerable(reason);
            ExitCode::from(code)
        }
    }
}

/// Why the self-host could not be compared against, and what to do.
fn report_unanswerable(reason: &SelfhostTerm) {
    match reason {
        SelfhostTerm::Stubbed => {
            eprintln!(
                "❌ the self-host binary has no diagnostic tools compiled in, so it \
                 rendered nothing"
            );
            eprintln!(
                "   the production build stubs them out (ADR 18.4.26f); rebuild with \
                 `make devcontainer-self-compile-dev`"
            );
        }
        SelfhostTerm::NotExecutable => {
            eprintln!("❌ the self-host binary could not be executed on this machine");
            eprintln!(
                "   `tungsten1` is built for the devcontainer's platform — run this \
                 comparison there:"
            );
            eprintln!("   devcontainer exec --workspace-folder . <this command>");
        }
        SelfhostTerm::Unsupported => {
            eprintln!(
                "❌ the self-host binary never ran the dump — it predates                  `--dump-core-terms`, or the check did not reach elaboration"
            );
            eprintln!(
                "   rebuild it with `make devcontainer-self-compile-dev`, or re-run                  with -v to see what it did print"
            );
        }
        SelfhostTerm::NotFound => {
            eprintln!("❌ the self-host has no definition by that name in this file");
            eprintln!(
                "   the two compilers disagree about what the file DEFINES, which is a \
                 bigger divergence than a term shape — compare with \
                 `tungsten diff bootstrap-selfhost-check`"
            );
        }
        SelfhostTerm::Unreadable => {
            eprintln!("❌ the self-host matched the definition but could not read its term back");
            eprintln!("   its arena handle did not materialise; re-run with -v");
        }
        SelfhostTerm::Rendered(_) => {
            eprintln!("ICE: a rendered term is not an unanswerable comparison");
        }
    }
}

/// A bounded slice of `text` starting at `index`, for pointing at a difference.
pub(super) fn tail_from(text: &str, index: usize) -> String {
    let start = text
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= index)
        .unwrap_or(text.len());
    text[start..].chars().take(48).collect()
}
