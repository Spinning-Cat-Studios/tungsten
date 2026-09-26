//! `tungsten info def`: definition type signature and Core IR inspection.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use std::fmt::Write as _;

use tungsten_core::builtins::interception_note;
use tungsten_core::terms::termination::{
    callers_of, describe_roots, OccurrenceGraph, RootCandidacy,
};

use crate::info::elaborate_for_info;
use crate::info::helpers::format_semantic_type;

/// The optional reports `info def` appends after the standard block.
///
/// A record rather than two more `bool` parameters: the standard block is
/// unconditional and these are opt-in additions, which is a concept the
/// signature should carry rather than a pair of positional flags a caller can
/// transpose silently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefReports {
    /// Each parameter's eligibility as a decreasing root (ADR 12.8.26a).
    pub why_not_certified: bool,
    /// Which definitions call this one (ADR 12.8.26b retrospective).
    pub callers: bool,
}

/// Show definition type signature, Core IR, free `TyVars`, and ADT references.
pub fn cmd_info_def(
    name: &str,
    file: &PathBuf,
    verbose: bool,
    max_errors: usize,
    reports: DefReports,
) -> ExitCode {
    let Some(project) = elaborate_for_info(file, verbose, max_errors) else {
        return ExitCode::FAILURE;
    };

    let Some(def) = project.defs.iter().find(|d| d.name == name) else {
        eprintln!("Definition not found: {name}");
        let mut available: Vec<&str> = project.defs.iter().map(|d| d.name.as_str()).collect();
        available.sort_unstable();
        if available.len() > 20 {
            eprintln!(
                "Available definitions ({} total): {}, ...",
                available.len(),
                available[..20].join(", ")
            );
        } else {
            eprintln!("Available definitions: {}", available.join(", "));
        }
        return ExitCode::FAILURE;
    };

    let structural = format!("{}", def.ty);
    let semantic = format_semantic_type(&def.ty, &project.type_provenance);

    println!("Definition: {name}");
    println!("{}", "═".repeat(12 + name.len()));
    println!();

    if let Some(ref sem) = semantic {
        println!("Type (semantic):    {sem}");
        println!("Type (structural):  {structural}");
    } else {
        println!("Type: {structural}");
    }
    println!();

    println!("Core IR:");
    println!("  {}", def.term);
    println!();

    let free = def.term.free_type_vars();
    let genuine: std::collections::HashSet<_> =
        free.into_iter().filter(|v| !v.starts_with('@')).collect();
    if genuine.is_empty() {
        println!("Free TyVars: ∅");
    } else {
        println!("Free TyVars: {genuine:?}");
    }

    // ADT references
    let ty_str = format!("{}", def.ty);
    let adt_refs: Vec<&str> = project
        .type_provenance
        .mu_origins
        .values()
        .filter(|o| ty_str.contains(&format!("α_{}", o.adt_name)))
        .map(|o| o.adt_name.as_str())
        .collect();
    if !adt_refs.is_empty() {
        let mut unique_refs: Vec<&str> = adt_refs;
        unique_refs.sort_unstable();
        unique_refs.dedup();
        println!("ADT references: {}", unique_refs.join(", "));
    }

    if reports.why_not_certified {
        println!();
        print!("{}", render_root_candidacy(&describe_roots(&def.term.term)));
    }

    if reports.callers {
        let def_map = project.defs.iter().map(|d| (d.name.as_str(), &d.term.term));
        let adjacency = OccurrenceGraph::build(def_map).adjacency();
        println!();
        print!("{}", render_callers(name, &callers_of(&adjacency, name)));
    }

    ExitCode::SUCCESS
}

/// The `--callers` block, as a value so it is assertable.
///
/// The verdict line is the point, not the list. "No callers" is the answer a
/// reader acts on, and it is the one grep cannot give: in `.tg` a `pub use`
/// re-export sits in a different file from both the definition and any call
/// site, so an exported-but-uncalled function greps exactly like a live one.
/// ADR 12.8.26b was written against such a function.
///
/// **`none` is a bootstrap-side answer, and for an intercepted name it is a true
/// answer to a question the reader is not asking** (ADR 20.8.26c D4). Ten bare
/// names are matched before name resolution, so a `.tg` function carrying one is
/// never resolved *here* — and reads as "dead, delete me" — while the
/// self-hosted compiler may resolve to it at every call site. The note below
/// fires at the moment someone is about to be misled, which is the one thing
/// prose in a doc comment cannot do.
#[must_use]
pub fn render_callers(name: &str, callers: &BTreeSet<String>) -> String {
    let others: Vec<&str> = callers
        .iter()
        .map(String::as_str)
        .filter(|caller| *caller != name)
        .collect();
    let self_recursive = callers.iter().any(|caller| caller == name);

    let mut out = String::new();
    if others.is_empty() {
        let _ = writeln!(
            out,
            "Callers: none — nothing in this project calls `{name}`{}",
            if self_recursive {
                ", and its only reference is its own recursion"
            } else {
                ""
            }
        );
        if let Some(note) = interception_note(name) {
            let _ = writeln!(out, "  warning: {note}");
            let _ = writeln!(
                out,
                "  see also: `tungsten info builtins {name}` — and do not delete this \
                 definition on the strength of `none`"
            );
            return out;
        }
        let _ = writeln!(
            out,
            "  note: an export is not a call. A `pub use` of `{name}` keeps it \
             compiling and visible while leaving it dead."
        );
        let _ = writeln!(
            out,
            "  see also: `tungsten doctor audit-dead-definitions <file>` for the whole census"
        );
        return out;
    }

    let _ = writeln!(out, "Callers ({}):", others.len());
    for caller in others {
        let _ = writeln!(out, "  {caller}");
    }
    if self_recursive {
        let _ = writeln!(out, "  (plus its own recursive call)");
    }
    out
}

/// The `--why-not-certified` block, as a value so it is assertable.
///
/// Says what a reader can *do*, not only what is wrong: a definition with no
/// eligible root will never certify however it is rewritten, and one with an
/// eligible root that still fails is a descent problem — a different fix, and
/// the whole reason the two cases are called out separately rather than left
/// for the reader to infer from a table.
#[must_use]
pub fn render_root_candidacy(roots: &[RootCandidacy]) -> String {
    if roots.is_empty() {
        return "Decreasing roots: none — the definition takes no parameters\n".to_string();
    }
    let mut out = String::from("Decreasing roots (ADR 12.8.26a):\n");
    for root in roots {
        let verdict = root.ineligible_because.unwrap_or("a candidate");
        let mark = if root.ineligible_because.is_none() {
            '✓'
        } else {
            '✗'
        };
        let _ = writeln!(
            out,
            "  {mark} {}: {}  — {verdict}",
            root.parameter, root.rendered_type
        );
    }
    let eligible = roots
        .iter()
        .filter(|r| r.ineligible_because.is_none())
        .count();
    if eligible == 0 {
        out.push_str(
            "  No parameter can be descended on, so this definition cannot be certified \
             by any rewrite of its body — change a parameter's type or mark it `#[partial]`.\n",
        );
    } else {
        let _ = writeln!(
            out,
            "  {eligible} candidate(s): if this still fails E0062, the root is fine and the \
             DESCENT is not — check what the recursive call passes."
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::render_root_candidacy;
    use tungsten_core::terms::termination::RootCandidacy;

    fn root(parameter: &str, ty: &str, because: Option<&'static str>) -> RootCandidacy {
        RootCandidacy {
            parameter: parameter.to_string(),
            rendered_type: ty.to_string(),
            ineligible_because: because,
        }
    }

    /// The two verdicts read differently, and the closing advice differs with
    /// them — asserted together because a renderer that always printed the
    /// "cannot be certified" line would pass a test of the ineligible case
    /// alone.
    #[test]
    fn an_eligible_root_and_an_ineligible_one_render_different_advice() {
        let eligible = render_root_candidacy(&[root("l", "μα. (Unit + α)", None)]);
        assert!(eligible.contains("✓ l: μα. (Unit + α)"), "{eligible}");
        assert!(eligible.contains("1 candidate(s)"), "{eligible}");
        assert!(
            eligible.contains("DESCENT is not"),
            "an eligible root redirects the reader to the call site — {eligible}"
        );

        let ineligible = render_root_candidacy(&[root("n", "Nat", Some("a primitive"))]);
        assert!(
            ineligible.contains("✗ n: Nat  — a primitive"),
            "{ineligible}"
        );
        assert!(
            ineligible.contains("cannot be certified by any rewrite"),
            "no eligible root is a dead end, and the advice must say so — {ineligible}"
        );
        assert!(
            !ineligible.contains("candidate(s)"),
            "the two closing lines are exclusive — {ineligible}"
        );
    }

    /// One eligible parameter among several is still one candidate: the count
    /// is of *candidates*, not of parameters. A renderer that printed
    /// `roots.len()` passes every single-parameter test.
    #[test]
    fn the_count_is_of_candidates_not_parameters() {
        let mixed = render_root_candidacy(&[
            root("st", "IRState", Some("a primitive")),
            root("e", "μα. Expr", None),
            root("n", "Nat", Some("a primitive")),
        ]);
        assert!(mixed.contains("1 candidate(s)"), "{mixed}");
    }

    #[test]
    fn a_nullary_definition_says_so_rather_than_printing_an_empty_table() {
        let none = render_root_candidacy(&[]);
        assert!(
            none.contains("none — the definition takes no parameters"),
            "{none}"
        );
    }
}

#[cfg(test)]
mod callers_tests {
    use super::render_callers;
    use std::collections::BTreeSet;

    fn set(of: &[&str]) -> BTreeSet<String> {
        of.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn no_callers_says_so_and_warns_that_an_export_is_not_one() {
        // The verdict a reader acts on, and the one grep cannot give.
        let out = render_callers("parse_items", &set(&[]));
        assert!(out.contains("Callers: none"), "{out}");
        assert!(out.contains("an export is not a call"), "{out}");
        assert!(out.contains("audit-dead-definitions"), "{out}");
    }

    #[test]
    fn callers_are_listed_and_counted() {
        let out = render_callers("helper", &set(&["main", "other"]));
        assert!(out.contains("Callers (2):"), "{out}");
        assert!(out.contains("  main"), "{out}");
        assert!(out.contains("  other"), "{out}");
    }

    #[test]
    fn a_purely_self_recursive_definition_still_reads_as_uncalled() {
        // Self-recursion is a caller, but not one that makes the definition
        // reachable — reporting "Callers (1): spin" would read as live.
        let out = render_callers("spin", &set(&["spin"]));
        assert!(out.contains("Callers: none"), "{out}");
        assert!(out.contains("its own recursion"), "{out}");
    }

    #[test]
    fn self_recursion_is_noted_beside_real_callers_not_counted_among_them() {
        let out = render_callers("walk", &set(&["main", "walk"]));
        assert!(out.contains("Callers (1):"), "counted itself: {out}");
        assert!(out.contains("  main"), "{out}");
        assert!(out.contains("plus its own recursive call"), "{out}");
    }

    #[test]
    fn the_uncalled_note_names_the_definition_it_is_about() {
        let out = render_callers("orphan", &set(&[]));
        assert!(out.contains("`orphan`"), "{out}");
    }

    #[test]
    fn a_called_definition_does_not_carry_the_dead_code_advice() {
        let out = render_callers("helper", &set(&["main"]));
        assert!(!out.contains("an export is not a call"), "{out}");
    }

    /// ADR 20.8.26c: for an intercepted name, `none` is a true answer to a
    /// question the reader is not asking, and the export note is the wrong
    /// advice — it invites the deletion the warning exists to stop.
    #[test]
    fn an_intercepted_name_gets_the_warning_instead_of_the_export_note() {
        let out = render_callers("string_len", &set(&[]));
        assert!(out.contains("Callers: none"), "{out}");
        assert!(out.contains("warning:"), "{out}");
        assert!(out.contains("before name resolution"), "{out}");
        assert!(out.contains("do not delete this definition"), "{out}");
        assert!(!out.contains("an export is not a call"), "{out}");
    }

    /// The complement, which is what makes the arm a *distinction* rather than
    /// a paragraph everyone learns to skip.
    #[test]
    fn an_unintercepted_name_is_unchanged_by_the_interception_arm() {
        let out = render_callers("lex_slice", &set(&[]));
        assert!(!out.contains("warning:"), "{out}");
        assert!(out.contains("an export is not a call"), "{out}");
    }

    /// A name both compilers intercept still warns — the definition is dead to
    /// both, which is a different sentence and still not "delete me on the
    /// strength of `none`".
    #[test]
    fn a_two_sided_intercepted_name_warns_too() {
        let out = render_callers("substring", &set(&[]));
        assert!(out.contains("warning:"), "{out}");
        assert!(out.contains("BOTH compilers"), "{out}");
    }
}
