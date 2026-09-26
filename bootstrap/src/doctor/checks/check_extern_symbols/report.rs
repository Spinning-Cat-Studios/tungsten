//! The pure half: classifying declarations against exports, and rendering.
//!
//! Kept separate from the walk so both the verdict and the wording are
//! assertable against injected data — no filesystem, no crate, no exit code.

use std::collections::BTreeMap;

use super::scan::{is_conditional, ExportedSymbol};
use crate::doctor::checks::check_extern_coverage::DeclaredExtern;

/// What one run concluded about a corpus of declarations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SymbolReport {
    /// Declarations naming a symbol `tungsten_core` exports unconditionally.
    pub resolved: Vec<DeclaredExtern>,
    /// Declarations whose only exports are `#[cfg]`-gated — they link on some
    /// targets and not others, carrying the gate that decides which.
    pub conditional: Vec<(DeclaredExtern, String)>,
    /// Declarations naming no export at all. **This is the finding**: an
    /// `undefined reference` at link time, minutes into a self-compile.
    pub unresolved: Vec<DeclaredExtern>,
}

impl SymbolReport {
    /// How many declarations were examined. Distinguishing this from "none
    /// unresolved" is the whole point: a run that examined nothing is not a
    /// run that found nothing.
    #[must_use]
    pub fn examined(&self) -> usize {
        self.resolved.len() + self.conditional.len() + self.unresolved.len()
    }

    /// Whether the run should turn the check red.
    #[must_use]
    pub fn has_findings(&self) -> bool {
        !self.unresolved.is_empty()
    }
}

/// What one run concluded, as a value rather than an exit code.
///
/// `ExitCode` implements no equality, so an exit decision made inline in the
/// command would be assertable only by spawning the binary — and every branch
/// of it would survive the mutation sweep. Three named arms rather than a bool
/// because the two non-clean outcomes mean different things to whoever reads
/// the run: one is a finding about the corpus, the other about the invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Every declaration resolved.
    Clean,
    /// At least one declaration names no export — an `undefined reference`
    /// waiting to happen.
    Unresolved,
    /// One side of the comparison was empty, so the run proves nothing. Red for
    /// the same reason a conformance gate over zero fixtures is red.
    ExaminedNothing,
}

impl Verdict {
    /// Whether the process should exit non-zero.
    #[must_use]
    pub(crate) fn is_failure(self) -> bool {
        self != Verdict::Clean
    }
}

/// Decide the run from what it found.
///
/// **Emptiness is checked first**, and on either side: a corpus with no
/// declarations and an export scan that found nothing both make every other
/// answer meaningless, and reporting "clean" for either is the failure this
/// check exists to prevent elsewhere.
#[must_use]
pub(crate) fn verdict_of(report: &SymbolReport, exports_scanned: usize) -> Verdict {
    if report.examined() == 0 || exports_scanned == 0 {
        Verdict::ExaminedNothing
    } else if report.has_findings() {
        Verdict::Unresolved
    } else {
        Verdict::Clean
    }
}

/// Classify every declaration against the export index.
///
/// A pure function of `(declared, exports)`, so the three-way split can be
/// asserted without a crate to scan.
#[must_use]
pub(crate) fn classify(
    declared: &[DeclaredExtern],
    exports: &BTreeMap<String, Vec<ExportedSymbol>>,
) -> SymbolReport {
    let mut report = SymbolReport::default();
    for declaration in declared {
        match exports.get(&declaration.symbol) {
            None => report.unresolved.push(declaration.clone()),
            Some(declarations) if is_conditional(declarations) => {
                let gates: Vec<&str> = declarations
                    .iter()
                    .filter_map(|export| export.cfg.as_deref())
                    .collect();
                report
                    .conditional
                    .push((declaration.clone(), gates.join(" / ")));
            }
            Some(_) => report.resolved.push(declaration.clone()),
        }
    }
    report
}

/// Render the report.
///
/// The **empty corpus is itself a finding**, not a clean run: a check whose
/// input resolved to nothing must not print the same "✓" as one that examined
/// 151 declarations and found them all sound. That confusion is how a gate
/// pointed at a moved path stays green.
#[must_use]
pub(crate) fn render(
    report: &SymbolReport,
    source: &str,
    exports_scanned: usize,
    verbose: bool,
) -> String {
    let mut out = String::new();

    if report.examined() == 0 {
        out.push_str(&format!(
            "⚠ No `extern \"C\"` declarations found in {source}\n\
             \n\
             Nothing was examined, so this run proves nothing. Check the path — a\n\
             file with no module tree below it reads exactly like a clean corpus.\n"
        ));
        return out;
    }
    if exports_scanned == 0 {
        out.push_str(
            "⚠ No exported symbols found in the tungsten_core source root\n\
             \n\
             Every declaration would be reported unresolved, which says more about\n\
             --core-root than about the file. Check the path.\n",
        );
        return out;
    }

    if report.unresolved.is_empty() {
        out.push_str(&format!(
            "✓ All {} declared extern(s) in {source} resolve to a tungsten_core export \
             ({exports_scanned} scanned)\n",
            report.examined()
        ));
    } else {
        out.push_str(&format!(
            "❌ {} of {} declared extern(s) in {source} resolve to NO tungsten_core export:\n\n",
            report.unresolved.len(),
            report.examined()
        ));
        for entry in &report.unresolved {
            out.push_str(&format!(
                "  {}  offset {}  in {}\n",
                entry.symbol, entry.offset, entry.file
            ));
        }
        out.push_str(
            "\nEach of these is an `undefined reference` at LINK time — several minutes\n\
             into a self-compile, long after the type-check that accepted the\n\
             declaration. An `extern \"C\"` declaration compiles on every target;\n\
             the missing symbol appears only when something tries to call it.\n\
             \n\
             Fix: add `#[no_mangle] pub extern \"C\" fn <symbol>` in `tungsten_core/src/ffi/`,\n\
             then `make devcontainer-build` BEFORE the next self-compile so the\n\
             container's libtungsten_core carries it.\n\
             \n\
             Use `tungsten doctor map-span <file> <offset>` for file:line:col.\n",
        );
    }

    if !report.conditional.is_empty() {
        out.push_str(&format!(
            "\n⚠ {} declaration(s) resolve only through a `#[cfg]`-gated export — present\n\
             on some targets, absent on others:\n\n",
            report.conditional.len()
        ));
        for (entry, gates) in &report.conditional {
            out.push_str(&format!("  {}  gated on {gates}\n", entry.symbol));
        }
        out.push_str(
            "\nReported, not gated: a green `cargo build --target <foo>` is not evidence\n\
             the artifact LOADS, and this is where that bites.\n",
        );
    }

    if verbose && !report.resolved.is_empty() {
        out.push_str("\nResolved:\n");
        for entry in &report.resolved {
            out.push_str(&format!("  {}  in {}\n", entry.symbol, entry.file));
        }
    }
    out
}
