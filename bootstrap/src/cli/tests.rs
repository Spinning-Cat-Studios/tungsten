//! Tests: every spelling ADR 19.8.26a moved still parses, grouped and flat.
//!
//! Driven off the real [`Cli`] root rather than a synthetic wrapper, because
//! the top-level namespace *is* the subject here — a `Commands` variant that
//! parses in isolation but collides at the root would pass a wrapper test.
//!
//! One table per direction, so a group added without its alias — or an alias
//! that silently lost an argument — fails here rather than in whichever caller
//! still spells it the old way. A hidden variant appears in no `--help` a human
//! reads, so a test is the only thing that keeps it true.

use clap::{CommandFactory, Parser};

use super::Cli;

fn parses(args: &[&str]) -> bool {
    Cli::try_parse_from(std::iter::once("tungsten").chain(args.iter().copied())).is_ok()
}

/// Grouped path ⇄ the flat spelling it replaced. Both must parse, forever.
const REGROUPED: &[(&[&str], &[&str])] = &[
    (&["expr", "eval", "1 + 2"], &["eval", "1 + 2"]),
    (&["expr", "repl"], &["repl"]),
    // `clean-project` is the one move that is also a RENAME, so a regression
    // here reads as "command not found" rather than as a relocated path.
    (
        &["cache", "clean-project", "main.tg"],
        &["clean", "main.tg"],
    ),
    (&["cache", "clean-project"], &["clean"]),
    (
        &["info", "type", "members", "constructors", "Option", "o.tg"],
        &["info", "type", "constructors", "Option", "o.tg"],
    ),
    (
        &["info", "type", "members", "field-type", "L.Cons.0", "l.tg"],
        &["info", "type", "field-type", "L.Cons.0", "l.tg"],
    ),
    (
        &["info", "type", "members", "record-fields", "Point", "h.tg"],
        &["info", "type", "record-fields", "Point", "h.tg"],
    ),
    (
        &["info", "type", "members", "visibility", "Token", "o.tg"],
        &["info", "type", "visibility", "Token", "o.tg"],
    ),
];

/// Spellings this ADR did NOT create, which must still be refused.
///
/// The table above is entirely positive, and a table of things that must
/// succeed cannot distinguish a working command tree from one that accepts
/// everything — `parses` hardwired to `true` satisfies every assertion in this
/// file, which is exactly what the mutation sweep found. So the helper's
/// contract is asserted in the other direction too, and the cases are chosen to
/// be the ones a regrouping can plausibly get wrong rather than arbitrary
/// garbage.
const MUST_NOT_PARSE: &[(&[&str], &str)] = &[
    // The re-home must MOVE `clean`, not copy it. A `clean-project` that also
    // resolved at top level would put that namespace straight back to 13 while
    // every functional test stayed green — the one regression `cli-surface`
    // would catch and nothing here would explain.
    //
    // The SECOND operand is load-bearing and must not be dropped: `tungsten`
    // carries an implicit `[FILE]` positional, so a bare `tungsten
    // clean-project` parses happily — as a request to check a file of that
    // name — and fails only when the driver cannot read it. One argument
    // asserts nothing; two make clap reject the extra and the refusal
    // observable.
    (
        &["clean-project", "main.tg"],
        "`clean-project` must exist only under `cache`",
    ),
    // `members` groups under `type`, not under `info`. The flat aliases this
    // ADR kept are the OLD spellings; inventing a new shorter one would teach a
    // path `--help` never shows.
    (
        &["info", "members", "constructors", "Option", "o.tg"],
        "`members` is a sub-namespace of `info type`, not of `info`",
    ),
    // A near-miss name inside a new namespace. If this parsed, the positive
    // table would be asserting nothing about the namespace's contents.
    (
        &["expr", "evaluate", "1 + 2"],
        "`expr` must not accept a subcommand it does not have",
    ),
    // `cache clean-project` takes at most one operand — the entry file whose
    // project cache to clear. Two would mean the re-home widened the surface.
    (
        &["cache", "clean-project", "a.tg", "b.tg"],
        "`cache clean-project` takes at most one entry file",
    ),
];

#[test]
fn both_spellings_parse_for_every_regrouped_command() {
    for (grouped, flat) in REGROUPED {
        assert!(parses(grouped), "grouped path stopped parsing: {grouped:?}");
        assert!(parses(flat), "hidden alias stopped parsing: {flat:?}");
    }
}

/// Without this, every other assertion in this file is satisfied by a command
/// tree that accepts anything at all.
#[test]
fn the_spellings_this_adr_did_not_create_are_still_refused() {
    for (args, why) in MUST_NOT_PARSE {
        assert!(!parses(args), "{why} — but {args:?} parsed");
    }
}

/// A flag on one spelling and not the other is the drift a parse-only table
/// would otherwise miss, since both halves still parse without it.
#[test]
fn flags_survive_the_move_on_both_spellings() {
    for (grouped, flat) in [
        (
            &[
                "info",
                "type",
                "members",
                "constructors",
                "L",
                "l.tg",
                "--raw",
            ][..],
            &["info", "type", "constructors", "L", "l.tg", "--raw"][..],
        ),
        (
            &["cache", "clean", "--dry-run"][..],
            &["cache", "clean", "--dry-run"][..],
        ),
    ] {
        assert!(
            parses(grouped),
            "flag lost on the grouped path: {grouped:?}"
        );
        assert!(parses(flat), "flag lost on the alias: {flat:?}");
    }
}

/// The aliases must stay *hidden*: a visible one would be counted by
/// `cli-surface` and hand back the headroom this ADR bought.
#[test]
fn every_alias_this_adr_kept_is_hidden_from_the_visible_surface() {
    let visible: Vec<String> = crate::list_commands::leaf_paths(&Cli::command());
    for gone in [
        "eval",
        "repl",
        "clean",
        "info type constructors",
        "info type field-type",
        "info type record-fields",
        "info type visibility",
    ] {
        assert!(
            !visible.contains(&gone.to_string()),
            "`{gone}` is still a VISIBLE leaf — it counts against the namespace \
             cap ADR 19.8.26a paid down, and `--help` now teaches two spellings"
        );
    }
}

/// The grouped paths, conversely, must be visible — an alias-only move would
/// leave the command reachable and undiscoverable at once.
#[test]
fn every_grouped_path_this_adr_added_is_visible() {
    let visible: Vec<String> = crate::list_commands::leaf_paths(&Cli::command());
    for added in [
        "expr eval",
        "expr repl",
        "cache clean-project",
        "info type members constructors",
        "info type members field-type",
        "info type members record-fields",
        "info type members visibility",
    ] {
        assert!(
            visible.contains(&added.to_string()),
            "`{added}` is not a visible leaf — `tungsten commands --tree` and \
             `--help` would not offer the spelling the surfaces now teach"
        );
    }
}
