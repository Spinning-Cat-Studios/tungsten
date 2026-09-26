//! The inventory reconciled against surfaces OUTSIDE the clap tree, and against
//! its own rendering.
//!
//! Split from the parent module at ADR 7.9.26c, which pushed it past the 400-line
//! `.rs` cap. The seam is the one the section comments already drew: everything
//! here reads a source of truth the clap tree does not hold — a command's
//! accepted flags, the `.mk` files' declared targets — or reads the inventory
//! back out through `render`/`json`. The parent keeps the two reconciliation
//! walks (D2.1 completeness, D2.2 no-phantoms) and the table's internal
//! consistency.

use crate::info::commands::pipeline::{json, render};

use super::{
    accepted_long_flags, all_entries, documented_flag_name, documented_subcommands, make_targets,
    EntryKind, SECTIONS, TREE_HAS_CODEGEN,
};

// ── D3 Flag: reconciled against the owning command's argument list ──

#[test]
fn every_documented_flag_is_accepted_by_the_command_that_owns_it() {
    let mut unaccepted: Vec<(&str, &str)> = Vec::new();
    let mut checked = 0_usize;
    for entry in all_entries() {
        let EntryKind::Flag { on } = entry.kind else {
            continue;
        };
        // A `compile` flag cannot be checked in the LLVM-free build, where the
        // subcommand itself is cfg'd out — the codegen arm covers those (D7).
        let Some(accepted) = accepted_long_flags(on) else {
            continue;
        };
        let Some(name) = documented_flag_name(entry.usage) else {
            unaccepted.push((on, entry.usage));
            continue;
        };
        checked += 1;
        if !accepted.contains(name.trim_start_matches("--")) {
            unaccepted.push((on, entry.usage));
        }
    }
    assert!(
        checked > 0,
        "no flag entry was checkable — a vacuous pass (ADR 2.7.26b T5a)"
    );
    assert!(
        unaccepted.is_empty(),
        "`tungsten info pipeline` advertises flags the owning command does not \
         accept — an agent following the inventory would pass a flag that \
         errors (codegen in this build: {TREE_HAS_CODEGEN}): {unaccepted:#?}"
    );
}

/// Closes the skip above: an unresolvable owner is legitimate only when the
/// command is cfg'd out, which nothing is in the codegen build. Without this, a
/// typo in `on` would silently disable the flag check for that entry — a gate
/// that reports green over an entry it never looked at.
#[cfg(feature = "codegen")]
#[test]
fn every_flag_owner_resolves_in_the_codegen_tree() {
    let mut unresolved: Vec<&str> = all_entries()
        .filter_map(|e| match e.kind {
            EntryKind::Flag { on } => Some(on),
            _ => None,
        })
        .filter(|on| accepted_long_flags(on).is_none())
        .collect();
    unresolved.sort_unstable();
    unresolved.dedup();
    assert!(
        unresolved.is_empty(),
        "these flag owners resolve to no command — likely a typo in `on`, which \
         would skip the flag check rather than fail it: {unresolved:#?}"
    );
}

// ── D3 MakeTarget: reconciled against the Makefile sources ──
// The extraction itself lives in `pipeline::reconcile::make_targets`, where it is
// unit tested against a synthetic makefile — a pure `&str -> HashSet<String>` is
// assertable in both directions (a comment and an indented recipe line are NOT
// targets), which "does the real Makefile contain check-health?" is not.

#[test]
fn every_documented_make_target_is_declared_in_the_makefile_sources() {
    // The inventory documents the development repository's targets, most of
    // them in the private make half the public repository does not carry
    // (ADR 25.9.26l D5). The reconciliation is against the full source set, so
    // it runs where that set exists — as `check_extern_coverage_tests` does for
    // `docs/repo-memory`.
    if !make_targets::private_make_present() {
        return;
    }
    let declared = make_targets::declared_make_targets();
    let documented: Vec<&str> = all_entries()
        .filter(|e| e.kind == EntryKind::MakeTarget)
        .map(|e| e.usage.trim_start_matches("make ").trim())
        .collect();
    assert!(
        !documented.is_empty(),
        "no make targets in the inventory — a vacuous pass (ADR 2.7.26b T5a)"
    );
    let mut phantoms: Vec<&str> = documented
        .into_iter()
        .filter(|name| !declared.contains(*name))
        .collect();
    phantoms.sort_unstable();
    assert!(
        phantoms.is_empty(),
        "`tungsten info pipeline` advertises make targets that no .mk file \
         declares: {phantoms:#?}"
    );
}

// ── The rendered surface ──

#[test]
fn the_rendered_listing_names_every_documented_subcommand() {
    let rendered = render::render(SECTIONS);
    for entry in documented_subcommands() {
        assert!(
            rendered.contains(entry.usage),
            "`{}` is in the inventory but not in the rendered listing",
            entry.path
        );
    }
}

#[test]
fn the_rendered_listing_has_no_line_of_only_whitespace() {
    // The hand-written listing lost a section's leading indentation to a `"\`
    // line continuation, and it took a human reading the output to notice
    // (ADR 28.7.26f §1.2). Trailing-whitespace lines are the same class.
    let rendered = render::render(SECTIONS);
    for (number, line) in rendered.lines().enumerate() {
        assert!(
            line.is_empty() || !line.trim().is_empty(),
            "line {} is whitespace-only: {line:?}",
            number + 1
        );
        assert_eq!(
            line.trim_end(),
            line,
            "line {} has trailing space",
            number + 1
        );
    }
}

#[test]
fn the_json_surface_emits_one_object_per_inventory_entry() {
    let emitted = json::render_json(SECTIONS);
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&emitted).expect("--json must emit valid JSON");
    assert_eq!(
        parsed.len(),
        all_entries().count(),
        "the JSON surface must be the whole inventory, not a truncation"
    );
    for object in &parsed {
        for key in [
            "path",
            "kind",
            "cost",
            "section",
            "usage",
            "requires_codegen",
        ] {
            assert!(object.get(key).is_some(), "entry missing `{key}`: {object}");
        }
    }
}

// ── ADR 18.9.26c AC 6: the `--alloc-profile` copy says the flag changes the shape ──

/// The phrase ADR 14.9.26b wrote and ADR 18.9.26c made false: unprofiled sites
/// now branch to `malloc` directly, so the flag DOES change what is emitted.
const RETIRED_ALLOC_PROFILE_CLAIM: &str = "no longer changes the allocation callee";

/// 18.9.26c AC 6 — the inventory overview entry.
#[test]
fn alloc_profile_inventory_entry_describes_the_emitted_shape_change() {
    let entry = all_entries()
        .find(|e| e.usage == "--alloc-profile[=fn]")
        .expect("the --alloc-profile flag has an inventory entry");
    assert!(
        !entry.summary.contains(RETIRED_ALLOC_PROFILE_CLAIM),
        "{}",
        entry.summary
    );
    for needle in ["18.9.26c", "SHAPE", "calls malloc directly"] {
        assert!(
            entry.summary.contains(needle),
            "missing {needle:?}: {}",
            entry.summary
        );
    }
}

/// 18.9.26c AC 6 — the `compile --alloc-profile` help, which exists only in
/// the codegen-featured build.
#[cfg(feature = "codegen")]
#[test]
fn alloc_profile_cli_help_describes_the_emitted_shape_change() {
    use clap::CommandFactory;
    let cli = crate::cli::Cli::command();
    let compile = cli
        .find_subcommand("compile")
        .expect("compile subcommand in the codegen build");
    let arg = compile
        .get_arguments()
        .find(|a| a.get_id() == "alloc_profile")
        .expect("compile --alloc-profile");
    let help = arg
        .get_long_help()
        .or_else(|| arg.get_help())
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(!help.contains("allocation callee is unchanged"), "{help}");
    for needle in [
        "18.9.26c",
        "changes the emitted allocation shape",
        "calls malloc",
    ] {
        assert!(help.contains(needle), "missing {needle:?}: {help}");
    }
}
