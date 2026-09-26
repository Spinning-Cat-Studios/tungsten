//! `tungsten doctor tool-reachability` — does each failure mode's companion
//! diagnostic still reach its own verdict? (ADR 12.8.26a)
//!
//! **The failure this exists for.** A `doctor check` that reports on some
//! property elaborates the file first, then prints its findings. When the
//! property's *gate* is made hard, elaboration aborts on exactly the files the
//! report is for — so the tool exits without printing anything, and the
//! diagnostic aimed at the failing corpus becomes unreachable on every failing
//! corpus. Nothing detects this: the subcommand still exists, still resolves,
//! still has the right cost tier, and its `--help` still describes the behaviour
//! it had before the flip.
//!
//! It has happened twice. ADR 7.8.26e made strict positivity a hard E0061 gate
//! and `doctor check type positivity` became unreachable on a non-positive file.
//! ADR 11.8.26b then made termination hard and did the same to
//! `doctor check type termination` — while all five AI surfaces, including the
//! subcommand's own `--help`, continued to promise it *was* reachable.
//!
//! **So this asserts agreement, not reachability.** Each pairing declares what
//! is expected, and the check fails when reality disagrees **in either
//! direction** — a tool that was supposed to be reachable and is not, and one
//! that was written off as blocked but has since been fixed. Only the second
//! direction keeps the table from rotting into a list of excuses.
//!
//! **The table is about failure modes, not only hard gates** (ADR 13.8.26c
//! §2.4). It began as E0061/E0062/E0064 — three unconditional gates — and the
//! word "gate" read like the entry condition. It is not: the invariant is that
//! *a diagnostic aimed at failing input must run on failing input*, and that
//! holds for any error that aborts elaboration. E0016 is an ordinary
//! elaboration error and aborts just as thoroughly, so narrowing the table to
//! hard gates would have excluded the pairing whose whole design turns on the
//! guarantee. The field is named `failure_mode` for that reason.
//!
//! # Scope: this covers the BOOTSTRAP's gates, and only those
//!
//! Every probe here calls a bootstrap function in-process —
//! `driver::elaborate_project`, or `census_of_file` for the parse-only one. No
//! probe spawns a compiler. So a gate that lives in the **self-hosted**
//! compiler cannot have a pairing in this table, and a green run says nothing
//! about it.
//!
//! That is a live limitation, not a hypothetical one. ADR 18.8.26b made strict
//! positivity a hard gate in `tungsten1` too, raising **E0700** where the
//! bootstrap raises E0061, and ADR 19.8.26d did the same for termination,
//! raising **E0710**/**E0711** where the bootstrap raises E0062/E0063 — three
//! self-hosted gates this check is blind to.
//!
//! **Do not add a row for a self-hosted gate.** It would probe the bootstrap
//! and report on the bootstrap, while reading as self-host coverage — which is
//! worse than the gap, because it converts an absence into a false assurance.
//! In all three cases the substance happens to be covered anyway: each
//! self-hosted code's companion is the *same* tool already asserted under its
//! bootstrap row — `doctor check type positivity` under E0061, `doctor check
//! type termination` under E0062 — and each pair is one rule over one shared
//! checker. That is reasoning, though, not a check.
//!
//! Closing the gap properly needs a probe that runs `./tungsten1`, which makes
//! this check depend on a self-compile — a real cost, and the reason it is
//! recorded here rather than bolted on. The self-hosted soundness gates are
//! meanwhile watched by `make selfcompiled-soundness-gate`, which compares both
//! compilers over the fixture corpus; that catches a gate that stops *firing*,
//! but not a companion diagnostic that stops being *reachable*.

use std::path::Path;
use std::process::ExitCode;

pub(crate) mod pairings;

pub use pairings::PAIRINGS;

/// Whether a companion tool can reach its verdict on input its gate rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachability {
    /// The tool elaborates the file and prints its report.
    Reachable,
    /// The gate aborts elaboration first, so the tool never reports.
    BlockedByGate,
}

impl std::fmt::Display for Reachability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Reachability::Reachable => "reachable",
            Reachability::BlockedByGate => "blocked by its gate",
        })
    }
}

/// One file of a pairing's fixture. The first in a pairing's list is the entry
/// file the probe is pointed at; the rest are siblings it resolves `mod` to.
///
/// A file *set* rather than one source because `ModDecl` is `mod foo;` with no
/// brace form, so a module-scoped failure — a name defined in two modules, say —
/// cannot be expressed in a single file at all (ADR 13.8.26c §2.4).
pub struct FixtureFile {
    /// Path relative to the pairing's temporary directory.
    pub path: &'static str,
    /// The file's contents.
    pub source: &'static str,
}

/// One (failure mode, companion diagnostic) pairing and what is expected of it.
pub struct CompanionPairing {
    /// The failure mode, as a user would name it — a hard gate or an ordinary
    /// elaboration error that aborts (see the module docs).
    pub failure_mode: &'static str,
    /// The companion diagnostic's command line.
    pub tool: &'static str,
    /// A source the failure mode rejects — the only input where reachability
    /// differs. The first entry is the entry file.
    pub fixture: &'static [FixtureFile],
    /// What the surfaces claim, and what this check holds them to.
    pub expected: Reachability,
    /// Run the companion's own elaboration path and report what happened.
    ///
    /// A function pointer rather than a flag because each companion decides
    /// its own enforcement: `termination` and `info` force `Report` for the
    /// elaboration they drive, `positivity` has no knob to force because E0061
    /// is unconditional, and `name-collisions` never elaborates at all.
    ///
    /// It reproduces the companion's *arrangement*; it does not call the
    /// companion. Deleting a guard from the tool while leaving it here would
    /// keep this check green — the end-to-end binding is
    /// `bootstrap/tests/termination_exit_codes.rs`, which spawns the binary.
    pub probe: fn(&Path) -> Reachability,
}

/// One pairing's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingOutcome {
    /// The tool's command line.
    pub tool: String,
    /// What the pairing promised.
    pub expected: Reachability,
    /// What actually happened.
    pub actual: Reachability,
}

impl PairingOutcome {
    /// Whether reality matched the promise.
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.expected == self.actual
    }
}

/// Run every pairing and collect the outcomes.
///
/// Takes the table so a test can drive it with its own pairings rather than
/// depending on the shipped ones — the shipped table is data that will change,
/// and a test asserting against it would have to change with it.
#[must_use]
pub fn evaluate(pairings: &[CompanionPairing]) -> Vec<PairingOutcome> {
    pairings
        .iter()
        .map(|pairing| {
            let dir = tempfile::tempdir().expect("create tempdir");
            let entry = materialize(dir.path(), pairing.fixture);
            PairingOutcome {
                tool: pairing.tool.to_string(),
                expected: pairing.expected,
                actual: (pairing.probe)(&entry),
            }
        })
        .collect()
}

/// Write a pairing's fixture into `dir` and return the entry file's path.
///
/// The first entry is the entry file by construction; an empty set is a
/// mis-declared pairing, and panicking beats probing a path that does not exist
/// and reporting the resulting `BlockedByGate` as a real drift.
fn materialize(dir: &Path, fixture: &[FixtureFile]) -> std::path::PathBuf {
    assert!(!fixture.is_empty(), "a pairing's fixture must name a file");
    for file in fixture {
        let path = dir.join(file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture directory");
        }
        std::fs::write(&path, file.source).expect("write fixture");
    }
    dir.join(fixture[0].path)
}

/// The command's whole output, as a value so every branch is assertable.
///
/// A `BlockedByGate` probe elaborates a file its gate rejects, so the gate
/// renders its own diagnostic to stderr on the way past. That output is the
/// evidence, not a failure — say so, because a green check that has just
/// printed `error: aborting due to 1 error` otherwise reads as broken and
/// trains its reader to stop believing the verdict line.
#[must_use]
pub fn render(outcomes: &[PairingOutcome]) -> String {
    let mut out = String::new();
    if outcomes
        .iter()
        .any(|outcome| outcome.actual == Reachability::BlockedByGate)
    {
        out.push_str(
            "  (a `blocked` pairing elaborates a rejected fixture, so its gate's own\n   diagnostic appears above — that is the probe working, not a failure)\n",
        );
    }
    for outcome in outcomes {
        let mark = if outcome.agrees() { "✓" } else { "✗" };
        out.push_str(&format!("  {mark} {}: {}\n", outcome.tool, outcome.actual));
        if !outcome.agrees() {
            out.push_str(&format!(
                "      expected {}, got {} — a gate change moved this tool's \
                 reachability without moving what its surfaces say (ADR 12.8.26a)\n",
                outcome.expected, outcome.actual
            ));
        }
    }
    let drifted = outcomes.iter().filter(|o| !o.agrees()).count();
    out.push_str(&if drifted == 0 {
        format!(
            "✓ {} companion diagnostic(s) reachable as documented\n",
            outcomes.len()
        )
    } else {
        format!(
            "✗ {drifted} of {} companion diagnostic(s) disagree with their documentation\n",
            outcomes.len()
        )
    });
    out
}

/// Whether every pairing matched what it declared.
///
/// Named rather than a `bool` so the decision is assertable: `ExitCode`
/// implements neither `PartialEq` nor any accessor, so a test on the command's
/// return value cannot distinguish success from failure at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReachabilityVerdict {
    /// Every pairing is as documented.
    AllAgree,
    /// At least one tool's reachability moved without its documentation.
    Drifted,
}

/// The verdict for a set of outcomes.
#[must_use]
pub fn verdict(outcomes: &[PairingOutcome]) -> ReachabilityVerdict {
    if outcomes.iter().all(PairingOutcome::agrees) {
        ReachabilityVerdict::AllAgree
    } else {
        ReachabilityVerdict::Drifted
    }
}

/// Entry point for `tungsten doctor tool-reachability`.
pub fn cmd_check_tool_reachability(_verbose: bool) -> ExitCode {
    let outcomes = evaluate(PAIRINGS);
    print!("{}", render(&outcomes));
    // Matched inline rather than through a `From` impl: that would be a *second*
    // ExitCode-returning function whose failure arm no in-process test can
    // reach, and one unkillable mapping is the documented residue here, not two
    // (`.claude/CLAUDE.md` § Killing survivors). The decision itself is in
    // `verdict`, which is an ordinary enum and is tested in both directions.
    match verdict(&outcomes) {
        ReachabilityVerdict::AllAgree => ExitCode::SUCCESS,
        ReachabilityVerdict::Drifted => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests;
