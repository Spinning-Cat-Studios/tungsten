//! Dead-definition census: which definitions no entry point reaches.
//!
//! Operates on the elaborated Core IR (no codegen), reusing the termination
//! gate's [`OccurrenceGraph`] so this and `audit-recursion` cannot disagree
//! about what a call is.
//!
//! # Why this is not "definitions with no callers"
//!
//! A mutually recursive pair nothing else calls has callers — each other — and
//! is still dead. So the census is **reachability from a declared root set**,
//! not an in-degree count. [`reachable_from`] does the walk; this module owns
//! the root *policy*, which is the part a reader has to be able to argue with.
//!
//! # The root policy, stated so it can be disagreed with
//!
//! A definition is a root if it is `main`, or if its name starts with `test_`
//! (the `tungsten test` discovery convention — those are called by the runner,
//! not by any definition in the file). `--root` adds more. The set is
//! **printed on every run**: a census whose roots are wrong reports confident
//! nonsense, and the only defence is showing the reader what it assumed.
//!
//! # What it does not know
//!
//! That a definition is exported for an outside consumer. `pub` is not a root
//! here, because in this repo the self-hosted compiler's `pub` surface is
//! internal — treating it as a root would make the census vacuous, which is the
//! failure ADR 12.8.26b's retrospective asked for this tool to avoid. A
//! genuinely external API needs `--root`.

mod render;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use tungsten_core::terms::termination::{unreachable_from, Adjacency, OccurrenceGraph};

use crate::driver;

pub use render::{render_census, DeadDefinitionCensus};

/// Names treated as entry points in addition to the policy defaults.
///
/// A free function over the definition names rather than a method, so the
/// policy is assertable without elaborating anything.
#[must_use]
pub fn roots_for(names: &BTreeSet<String>, extra: &[String]) -> BTreeSet<String> {
    let mut roots: BTreeSet<String> = names
        .iter()
        .filter(|name| is_default_root(name))
        .cloned()
        .collect();
    roots.extend(extra.iter().cloned());
    roots
}

/// Whether `name` is an entry point by the default policy.
#[must_use]
pub fn is_default_root(name: &str) -> bool {
    // `test_` rather than a `test` prefix: `tester` is an ordinary definition,
    // and admitting it as a root would silently hide everything it reaches.
    name == "main" || name.starts_with("test_")
}

/// The census itself: root policy plus reachability, over an adjacency.
///
/// Split from the command so the whole decision is a pure function of
/// `(adjacency, extra_roots)` — the command around it does nothing but
/// elaborate, build the graph and print, none of which a test can assert on
/// without a source file.
#[must_use]
pub fn census_of(adjacency: &Adjacency, extra_roots: &[String]) -> DeadDefinitionCensus {
    let names: BTreeSet<String> = adjacency.keys().cloned().collect();
    let roots = roots_for(&names, extra_roots);
    let dead = unreachable_from(adjacency, &roots);
    DeadDefinitionCensus {
        examined: names.len(),
        roots,
        dead,
    }
}

/// Run the dead-definition census.
pub fn cmd_audit_dead_definitions(
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    extra_roots: &[String],
) -> ExitCode {
    let project = match driver::elaborate_project(file, verbose, max_errors, None) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let graph = OccurrenceGraph::build(
        project
            .defs
            .iter()
            .map(|def| (def.name.as_str(), &def.term.term)),
    );
    print!(
        "{}",
        render_census(&census_of(&graph.adjacency(), extra_roots))
    );

    // Reporting, never gating. Dead code is a finding to weigh, not a build
    // break — and a census that failed the build would be silenced within a
    // week, which is worse than one nobody has to argue with.
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests;
