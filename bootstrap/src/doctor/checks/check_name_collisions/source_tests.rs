//! Tests: the parse-side half — the `extern "C"` walk and the command's two
//! outcomes.
//!
//! These write a real module tree to a temporary directory, because the two
//! things they assert cannot be reached from a hand-built `ModuleInfo`:
//! `ModuleContents` records visibility but *not* extern-ness, so the extern
//! walk has no seam short of an AST; and the parse failure that produces
//! `Outcome::Unparsable` needs a file to fail on.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::census::{CollisionClass, ReexportHandling, Severity};
use super::{census_of_tree, run, Outcome};
use crate::driver::modules::parse_module_tree;

/// Write a module tree and return its directory and entry file.
fn tree(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create tempdir");
    for (name, source) in files {
        std::fs::write(dir.path().join(name), source).expect("write fixture");
    }
    let entry = dir.path().join(files[0].0);
    (dir, entry)
}

/// Census a written tree the way the command does.
fn census_of(entry: &Path) -> super::census::Census {
    let mut visited = std::collections::HashSet::new();
    let mut chain = Vec::new();
    let parsed = parse_module_tree(entry, &mut visited, &mut chain, None).expect("parses");
    census_of_tree(&parsed, ReexportHandling::Subtract)
}

/// D3 class (c): two `extern "C" fn` of one name. The Tungsten identifier IS
/// the C symbol (ADR 7.8.26b D3), so this is a duplicate *link* symbol as well
/// as a shadowed binding, and no visibility change fixes it.
///
/// This is the only test of the extern walk, and it is a real one: extern-ness
/// comes from the AST, so a walk that returned nothing would silently demote
/// this finding to `private-shadowed` — the wrong remedy, confidently printed.
#[test]
fn two_extern_c_definitions_of_one_name_are_the_link_class() {
    let (_dir, entry) = tree(&[
        ("main.tg", "mod alpha;\nmod beta;\nfn main() -> Nat { 0 }\n"),
        (
            "alpha.tg",
            "pub extern \"C\" fn tg_report(n: Nat) -> Nat;\n",
        ),
        ("beta.tg", "pub extern \"C\" fn tg_report(n: Nat) -> Nat;\n"),
    ]);

    let census = census_of(&entry);

    let finding = census
        .collisions
        .iter()
        .find(|c| c.name == "tg_report")
        .expect("the extern pair is a finding");
    assert_eq!(finding.class, CollisionClass::ExternSymbol);
    assert!(
        finding.sites.iter().all(|site| site.is_extern_c),
        "both sites are extern: {:?}",
        finding.sites
    );
}

/// The same two definitions as plain `fn`s are NOT the link class — which is
/// what makes the test above an assertion about the extern walk rather than
/// about the collision.
#[test]
fn the_same_pair_as_plain_fns_is_not_the_link_class() {
    let (_dir, entry) = tree(&[
        ("main.tg", "mod alpha;\nmod beta;\nfn main() -> Nat { 0 }\n"),
        ("alpha.tg", "pub fn tg_report(n: Nat) -> Nat { n }\n"),
        ("beta.tg", "pub fn tg_report(n: Nat) -> Nat { n }\n"),
    ]);

    let finding = census_of(&entry)
        .collisions
        .into_iter()
        .find(|c| c.name == "tg_report")
        .expect("still a collision");
    assert_ne!(finding.class, CollisionClass::ExternSymbol);
    assert!(finding.sites.iter().all(|site| !site.is_extern_c));
}

/// `--json` reaches the JSON renderer through `run`. The renderer is unit
/// tested directly, but nothing else drives the flag from the command's own
/// entry point — and the flag is D3's whole gating story: "a caller wanting a
/// gate reads `--json`".
#[test]
fn the_json_flag_reaches_the_json_renderer() {
    let (_dir, entry) = tree(&[
        ("main.tg", "mod alpha;\nmod beta;\nfn main() -> Nat { 0 }\n"),
        ("alpha.tg", "fn shared() -> Nat { 1 }\n"),
        ("beta.tg", "fn shared() -> Nat { 2 }\n"),
    ]);

    let Outcome::Reported(human) = run(&entry, Severity::All, false, false) else {
        panic!("a parsable tree reports");
    };
    let Outcome::Reported(json) = run(&entry, Severity::All, true, false) else {
        panic!("a parsable tree reports");
    };
    assert_ne!(human, json, "the flag selects a different renderer");

    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["collision_count"], 1);
    assert_eq!(parsed["collisions"][0]["name"], "shared");
    assert!(
        serde_json::from_str::<serde_json::Value>(&human).is_err(),
        "and the default is emphatically not JSON"
    );
}

/// `--include-reexports` reaches the measurement arm through `run` too, so the
/// two runs AC 1 compares are genuinely two runs.
#[test]
fn the_include_reexports_flag_reaches_the_measurement_arm() {
    let (_dir, entry) = tree(&[
        ("main.tg", "mod alpha;\nmod beta;\nfn main() -> Nat { 0 }\n"),
        ("alpha.tg", "pub fn shared() -> Nat { 1 }\n"),
        ("beta.tg", "pub use alpha::shared;\n"),
    ]);

    let subtracted = run(&entry, Severity::All, true, false);
    let kept = run(&entry, Severity::All, true, true);
    assert_ne!(
        subtracted, kept,
        "the re-exported copy is subtracted by default and kept by the flag"
    );
}

/// A tree that parses reports, findings or not — the check is advisory (D3).
#[test]
fn a_parsable_tree_reports() {
    let (_dir, entry) = tree(&[("main.tg", "fn main() -> Nat { 0 }\n")]);

    let Outcome::Reported(report) = run(&entry, Severity::All, false, false) else {
        panic!("a parsable tree reports");
    };
    assert!(report.contains("examined 1 module(s)"));
}

/// A file that cannot be parsed at all is **bad input**, not a clean run — the
/// one thing this command exits non-zero for.
#[test]
fn an_unparsable_entry_file_is_bad_input_not_a_clean_run() {
    let outcome = run(
        Path::new("/nonexistent/definitely-not-here.tg"),
        Severity::All,
        false,
        false,
    );
    let Outcome::Unparsable(message) = &outcome else {
        panic!("expected bad input, got {outcome:?}");
    };
    assert!(message.starts_with("error: "), "{message}");
    assert_ne!(
        outcome,
        run(
            &tree(&[("main.tg", "fn main() -> Nat { 0 }\n")]).1,
            Severity::All,
            false,
            false
        ),
        "bad input and a clean run are different outcomes"
    );
}
