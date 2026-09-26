//! The driver-reach partition, as a value, plus its rendering.
//!
//! Split from the walk so the whole report is assertable without parsing a
//! source tree — and so the two ways this report can be vacuous (nothing
//! examined; nothing to compare against) each have a line of their own.

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// What one `audit-driver-reach` run found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DriverReach {
    /// Modules in the union of every entry file's tree — the reach line's
    /// denominator.
    pub examined: usize,
    /// The entry file the driver starts from, as given.
    pub driver_entry: String,
    /// The `test_*` / `mustfail_*` entry files the walk also rooted at.
    pub test_entries: BTreeSet<String>,
    /// Entry files that were named but could not be parsed. Named rather than
    /// dropped: a skipped test entry shrinks `test_only` silently, which is the
    /// exact shape of a report that looks clean because it read nothing.
    pub unreadable_entries: BTreeSet<String>,
    /// Modules the driver entry reaches through `use` edges.
    pub driver_reached: BTreeSet<String>,
    /// Modules some test entry reaches and the driver entry does not.
    pub test_only: BTreeSet<String>,
    /// Modules declared in some tree that no entry file reaches.
    pub unreached: BTreeSet<String>,
}

/// Render the partition.
///
/// **The reach line is not decoration, and neither is the no-test-entry note.**
/// This report has two independent ways to be empty and they mean opposite
/// things: `0 test-only` over 284 modules and 22 entry files is a *result*,
/// while `0 test-only` because no test entry file was read is a run that proves
/// nothing. Without a line for each they render identically.
#[must_use]
pub fn render_reach(reach: &DriverReach) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "reach: {} module(s) examined across {} entry file(s) (1 driver, {} test)",
        reach.examined,
        reach.test_entries.len() + 1,
        reach.test_entries.len()
    );
    let _ = writeln!(out, "driver entry: {}", reach.driver_entry);

    render_entry_warnings(&mut out, reach);

    if reach.examined == 0 {
        let _ = writeln!(
            out,
            "  ** no module was examined — this run proves nothing about any \
             subsystem **"
        );
        return out;
    }

    let _ = writeln!(
        out,
        "\ndriver-reached: {} module(s)",
        reach.driver_reached.len()
    );
    render_class(
        &mut out,
        "test-only",
        &reach.test_only,
        "reached from a test entry file and from no driver path — compiled and \
         type-checked on every build, executed only by the suite below",
    );
    render_class(
        &mut out,
        "reached by neither",
        &reach.unreached,
        "declared in a module tree and imported by nothing — usually an in-tree \
         `test_*` module, whose definitions the runner calls directly",
    );

    let _ = writeln!(
        out,
        "\nnote: reach here is the module-level `use` graph. A `mod` declaration \
         is NOT reach — `src/compiler/main.tg` declares `mod codegen;` and no \
         driver path ever imports it, which is why the module tree cannot answer \
         this question (ADR 3.9.26a)."
    );
    let _ = writeln!(
        out,
        "      Reports, never gates: a module that is test-only today and \
         driver-reached tomorrow is progress."
    );
    let _ = writeln!(
        out,
        "      inspect one with `tungsten info module imports <module> <file>`; \
         add an entry file with `--test-entry <path>`."
    );
    out
}

/// Warn about the two ways the entry-file set can make the report vacuous.
fn render_entry_warnings(out: &mut String, reach: &DriverReach) {
    if reach.test_entries.is_empty() {
        let _ = writeln!(
            out,
            "  ** no test entry file was read — \"test-only\" below is empty \
             because nothing was compared against, not because every module is \
             driver-reached **"
        );
    } else {
        let listed: Vec<&str> = reach.test_entries.iter().map(String::as_str).collect();
        let _ = writeln!(out, "test entries: {}", listed.join(", "));
    }

    if !reach.unreadable_entries.is_empty() {
        let listed: Vec<&str> = reach
            .unreadable_entries
            .iter()
            .map(String::as_str)
            .collect();
        let _ = writeln!(
            out,
            "  ** {} entry file(s) could not be parsed and contributed nothing: {} **",
            reach.unreadable_entries.len(),
            listed.join(", ")
        );
    }
}

/// Render one partition class, or say explicitly that it is empty.
fn render_class(out: &mut String, label: &str, members: &BTreeSet<String>, gloss: &str) {
    if members.is_empty() {
        let _ = writeln!(out, "\n{label}: none");
        return;
    }
    let _ = writeln!(out, "\n{label}: {} module(s)", members.len());
    let _ = writeln!(out, "  ({gloss})");
    for name in members {
        let _ = writeln!(out, "  {name}");
    }
}
