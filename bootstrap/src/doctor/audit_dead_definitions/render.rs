//! Rendering the census, as a value so it is assertable.

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// What one census run found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadDefinitionCensus {
    /// Definitions in the call graph — the reach line's denominator.
    pub examined: usize,
    /// The entry points the walk started from.
    pub roots: BTreeSet<String>,
    /// Definitions no root reaches.
    pub dead: BTreeSet<String>,
}

/// Render the census.
///
/// **The reach line and the root list are not decoration.** `0 dead` over 0
/// definitions and `0 dead` over 2,207 render identically without them, and a
/// census that walked from the wrong roots is confidently wrong rather than
/// visibly empty — the failure this repo keeps rediscovering in gates that
/// examined nothing.
#[must_use]
pub fn render_census(census: &DeadDefinitionCensus) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "reach: {} definition(s) examined, {} root(s)",
        census.examined,
        census.roots.len()
    );

    if census.roots.is_empty() {
        let _ = writeln!(
            out,
            "  ** no entry point found — every definition below is \"unreachable\" \
             because the walk started nowhere, not because it is dead **"
        );
    } else {
        let listed: Vec<&str> = census.roots.iter().map(String::as_str).collect();
        let _ = writeln!(out, "roots: {}", listed.join(", "));
    }

    if census.examined == 0 {
        let _ = writeln!(
            out,
            "  ** nothing was examined — this run proves nothing **"
        );
        return out;
    }

    if census.dead.is_empty() {
        let _ = writeln!(out, "✓ every definition is reachable from a root");
        return out;
    }

    let _ = writeln!(out, "\n{} unreachable definition(s):", census.dead.len());
    for name in &census.dead {
        // A name the bootstrap intercepts before name resolution is dead HERE
        // and may be live in the self-host, so it is marked in place rather than
        // in a footnote (ADR 20.8.26c D4): the list is what gets skimmed, and an
        // unmarked entry reads as a deletion candidate.
        if tungsten_core::builtins::is_bootstrap_intercepted(name) {
            let _ = writeln!(out, "  {name}   ** intercepted builtin — see below **");
        } else {
            let _ = writeln!(out, "  {name}");
        }
    }
    if census
        .dead
        .iter()
        .any(|name| tungsten_core::builtins::is_bootstrap_intercepted(name))
    {
        let _ = writeln!(
            out,
            "\nwarning: a marked name above is intercepted as a builtin BEFORE name \
             resolution, so the bootstrap never resolves to that definition and this \
             census cannot see whether anything uses it. The self-hosted compiler may \
             resolve to it at every call site. Check `tungsten info builtins <name>` \
             before deleting one."
        );
    }
    let _ = writeln!(
        out,
        "\nnote: an export is not a call — a `pub use`, or a `use` that only imports \
         a name, keeps these compiling and visible while leaving them uncalled here."
    );
    let _ = writeln!(
        out,
        "      THIS CENSUS IS PER ENTRY FILE. A definition reached only from a \
         DIFFERENT entry file — a `test_*.tg` suite, another binary — is unreachable \
         from this one and correctly listed. Confirm with `--callers` before deleting \
         anything."
    );
    let _ = writeln!(
        out,
        "      inspect one with `tungsten info def <name> <file> --callers`; add an \
         entry point with `--root <name>`."
    );
    out
}
