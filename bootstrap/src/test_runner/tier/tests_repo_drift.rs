// Tests: tier/mod.rs — the two drift guards expressed against the REPO
// checkout, split out of tests.rs by ADR 31.7.26c (that file reached its
// 400-line cap when the makefile guard was widened to the whole make/ tree).
//
// The seam is real, not arithmetic: everything in tests.rs asserts tier
// resolution over literal input and is therefore mutation-visible, while both
// tests here read the checkout and self-skip in a copied workspace.
use super::*;

use crate::info::commands::pipeline::reconcile::make_targets::makefile_sources;

// ---------------------------------------------------------------------------

/// The repo's own manifest parses, and its declarations agree with the files
/// they name.
///
/// Expressed against the repo checkout, so it is skipped when the sweep runs
/// in a copied workspace without it — the guards themselves are asserted above
/// over literal inputs, which is what makes them mutation-visible.
#[test]
fn the_shipped_manifest_declares_every_globbed_file_consistently() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let manifest_path = root.join(MANIFEST_FILENAME);
    if !manifest_path.is_file() {
        return;
    }
    let manifest = TierManifest::parse_at(&manifest_path).expect("the shipped manifest must parse");

    let mut checked = 0;
    for entry in std::fs::read_dir(root.join("src/compiler")).expect("src/compiler must exist") {
        let path = entry.expect("readable dir entry").path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("test_") || !name.ends_with(".tg") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("readable test file");
        let tier = manifest
            .tier_for(&path, &source)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            tier.is_some(),
            "{name} matches must_declare, so it must resolve to a tier"
        );
        checked += 1;
    }
    assert!(
        checked >= 16,
        "expected the whole src/compiler test corpus, saw {checked} file(s)"
    );

    // Every `why` is filled in: a bare tier number is a claim nobody can check.
    for (key, declaration) in &manifest.parsed.files {
        assert!(
            !declaration.why.trim().is_empty(),
            "{key} declares a tier with no reason"
        );
    }

    // Every permitted failure in the SHIPPED manifest names an owner. An entry
    // whose `adr` were blank would be a disabled test with nobody's name on it
    // — the thing D6 exists to prevent — so the owner is what is asserted.
    //
    // Deliberately a property over whatever the manifest holds, not a count.
    // This test used to pin `test_codegen.tg` at exactly 6, which made
    // *emptying* the block fail the suite: 7.8.26c registered the type arena,
    // those six started passing, `tungsten test` demanded the entry be deleted
    // (as designed), and deleting it then broke this. A manifest with no
    // expected failures at all is the healthy state, and it must not read as a
    // regression here.
    let mut permitted_total = 0;
    for entry in std::fs::read_dir(root.join("src/compiler")).expect("src/compiler must exist") {
        let path = entry.expect("readable dir entry").path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("test_") || !name.ends_with(".tg") {
            continue;
        }
        for (test, owner) in manifest.expected_failures(&path) {
            assert!(
                !owner.trim().is_empty(),
                "{name}: {test} is permitted to fail but names no owning ADR"
            );
            permitted_total += 1;
        }
    }

    // A file with no block gets no permissions — without this, the loop above
    // would be satisfied by a lookup that ignored its argument entirely and
    // returned nothing for everything.
    assert!(
        manifest
            .expected_failures(&root.join("src/compiler/test_strmap.tg"))
            .is_empty(),
        "a file with no expected_failure block must have none"
    );

    // Recorded rather than bounded: the ownership property above is the gate,
    // and this is the number a reader wants when it fires.
    eprintln!("shipped manifest permits {permitted_total} expected failure(s)");
}

/// The tier is declared in ONE place (D4), checked rather than asserted in
/// prose: no `make` recipe may hand `--check-only` to `tungsten test`.
///
/// Scoped to recipe lines on purpose — the surrounding prose comments still
/// mention the flag, and they are worth keeping. Like the test above this is
/// vacuous in the mutation sweep's copied workspace; the drift it guards
/// against is a repo fact, not a function's behaviour.
///
/// Scans EVERY makefile source rather than one named file (ADR 31.7.26c). It
/// used to read `make/quality.mk` alone, which is where the `tg-test` recipes
/// lived — so the split that moved them to `make/quality/tg-tests.mk` would
/// have left this test reading a file with no `tungsten test` recipe in it at
/// all and passing, forever, over an invariant it had stopped checking. A
/// per-file path is the wrong shape for a "nowhere in the build system" rule.
///
/// The file list comes from the `info pipeline` reconciler's
/// [`makefile_sources`], the one place that already answers "which files are
/// this repo's build system?". The close-out review caught this test carrying a
/// second, subtly different copy — which omitted the root `Makefile` entirely,
/// so a `--check-only` recipe there would have gone unseen by a rule whose
/// whole claim is "nowhere".
#[test]
fn no_recipe_line_declares_a_cost_tier_inline() {
    if !Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../make")
        .is_dir()
    {
        return;
    }
    let makefiles = makefile_sources();
    assert!(
        !makefiles.is_empty(),
        "no makefile sources discovered — this test would pass vacuously"
    );
    // Discovery is asserted, not assumed. Scanning the WRONG files reads exactly
    // like a clean repo: every non-recipe file trivially contains no
    // `--check-only` line, so a broken extension filter or a lost descend would
    // leave this test green over nothing. A mutation sweep made the point —
    // flipping `== "mk"` to `!=` survived until assertions like these existed.
    // Both ends of the tree are named, because the two ways this list can go
    // wrong are losing the root and losing the descend.
    assert!(
        makefiles.iter().any(|p| p.ends_with("Makefile")),
        "the root Makefile is a recipe source too: {makefiles:?}"
    );
    assert!(
        makefiles.iter().any(|p| p.ends_with("quality/tg-tests.mk")),
        "the `tungsten test` recipes live in a make/quality/ fragment (ADR \
         31.7.26c) — if the descend stops finding it, the rule below is \
         unenforced: {makefiles:?}"
    );
    let mut offenders: Vec<String> = Vec::new();
    for path in &makefiles {
        let text = std::fs::read_to_string(path).expect("makefile must be readable");
        offenders.extend(
            text.lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .filter(|line| line.contains("-- test ") && line.contains("--check-only"))
                .map(|line| format!("{}: {line}", path.display())),
        );
    }
    assert!(
        offenders.is_empty(),
        "the cost tier belongs in {MANIFEST_FILENAME} alone, but these recipe \
         lines declare it inline as well (ADR 6.8.26c D4):\n{}",
        offenders.join("\n")
    );
}
