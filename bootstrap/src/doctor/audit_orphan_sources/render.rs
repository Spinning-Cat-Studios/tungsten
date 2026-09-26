//! The orphan-source census as a value, plus its rendering.
//!
//! Split from the walk so the whole report is assertable without a source tree
//! on disk — and so the three ways this report can be vacuous each get a line
//! of their own: nothing walked, nothing subtracted, nothing scanned for build
//! mentions. Each of them empties `stranded` for a different reason, and
//! without a line apiece they render exactly like a clean tree.

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// What one `audit-orphan-sources` run found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrphanSources {
    /// `.tg` files found under the walk root — the reach line's denominator.
    pub files_walked: usize,
    /// Entry files whose module tree was parsed and subtracted.
    pub entry_files_read: usize,
    /// Distinct module source files those trees declared.
    pub modules_subtracted: usize,
    /// Build files read when deciding the build-swapped class.
    pub build_files_scanned: usize,
    /// Entry files that were named but could not be parsed.
    pub unreadable_entries: BTreeSet<String>,
    /// Undeclared, and named by nothing on disk — the finding.
    pub stranded: BTreeSet<String>,
    /// Undeclared, and copied into place by a build recipe (D2).
    pub build_swapped: BTreeSet<String>,
    /// Undeclared because they are roots: `main.tg`, `test_*.tg`,
    /// `mustfail_*.tg`.
    pub entry_roots: BTreeSet<String>,
}

/// Render the census.
///
/// **The reach line is not decoration.** `0 stranded` over 310 files and 24
/// entry files is a result; `0 stranded` because the walk read nothing, or
/// because no build file was scanned, is a run that proves nothing. The three
/// warnings below are what keeps those apart (AC 3).
#[must_use]
pub fn render_orphans(sources: &OrphanSources) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "reach: {} .tg file(s) walked, {} entry file(s) read, {} module(s) \
         subtracted, {} build file(s) scanned",
        sources.files_walked,
        sources.entry_files_read,
        sources.modules_subtracted,
        sources.build_files_scanned
    );

    render_vacuity_warnings(&mut out, sources);

    if sources.files_walked == 0 {
        let _ = writeln!(
            out,
            "  ** no .tg file was walked — this run proves nothing about any \
             file on disk **"
        );
        return out;
    }

    render_class(
        &mut out,
        "stranded",
        &sources.stranded,
        "on disk, declared by no `mod` statement, and named by no build file — \
         compiled by nothing, run by nothing, and reported by nothing else",
    );
    render_class(
        &mut out,
        "build-swapped",
        &sources.build_swapped,
        "declared by no `mod` statement, and copied over another file by a \
         build recipe — undeclared by design, not debt",
    );
    render_class(
        &mut out,
        "entry files",
        &sources.entry_roots,
        "roots rather than modules, so nothing declares them and nothing \
         should — `main.tg`, `test_*.tg`, `mustfail_*.tg`",
    );

    let _ = writeln!(
        out,
        "\nnote: the comparison is the filesystem MINUS the module tree. Every \
         other reachability tool starts from an entry file and walks \
         declarations, so none of them can see a file no `mod` statement names \
         (ADR 3.9.26q)."
    );
    let _ = writeln!(
        out,
        "      Reports, never gates: a file can be legitimately undeclared \
         while it is being written, and whether a stranded file should be \
         deleted or wired up is a judgement."
    );
    let _ = writeln!(
        out,
        "      inspect the declared side with `tungsten info module tree \
         <file>`; partition the modules that ARE declared with `tungsten \
         doctor audit-driver-reach <file>`."
    );
    out
}

/// Warn about the ways the inputs can make the census vacuous.
fn render_vacuity_warnings(out: &mut String, sources: &OrphanSources) {
    if sources.modules_subtracted == 0 {
        let _ = writeln!(
            out,
            "  ** no module was subtracted — every file below is \"undeclared\" \
             because no module tree was read, not because nothing declares \
             them **"
        );
    }
    if sources.build_files_scanned == 0 {
        let _ = writeln!(
            out,
            "  ** no build file was scanned — \"build-swapped\" below is empty \
             because nothing was compared against, so a copy-over template \
             will be reported as stranded **"
        );
    }
    if !sources.unreadable_entries.is_empty() {
        let listed: Vec<&str> = sources
            .unreadable_entries
            .iter()
            .map(String::as_str)
            .collect();
        let _ = writeln!(
            out,
            "  ** {} entry file(s) could not be parsed and subtracted nothing: \
             {} **",
            sources.unreadable_entries.len(),
            listed.join(", ")
        );
    }
}

/// Render one class, or say explicitly that it is empty.
fn render_class(out: &mut String, label: &str, members: &BTreeSet<String>, gloss: &str) {
    if members.is_empty() {
        let _ = writeln!(out, "\n{label}: none");
        return;
    }
    let _ = writeln!(out, "\n{label}: {} file(s)", members.len());
    let _ = writeln!(out, "  ({gloss})");
    for name in members {
        let _ = writeln!(out, "  {name}");
    }
}
