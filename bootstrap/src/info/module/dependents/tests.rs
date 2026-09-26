//! Tests for `info module dependents` (ADR 5.9.26f) — this file's own surface,
//! plus the fixture its siblings share.
//!
//! The fixture is a six-module tree written to a tempdir: `driver::ffi::types`
//! is the target, `driver::ffi` glob-re-exports it, two consumers import the
//! same item by the two paths a regroup has to tell apart, and `stray` carries
//! the two references that must land in neither column.

use super::*;

/// A parsed fixture tree plus the tempdir keeping its files alive.
pub struct Fixture {
    /// Dropped last: the tempdir the sources live in.
    _dir: tempfile::TempDir,
    pub tree: ParsedModule,
    pub info: ModuleInfo,
    pub target: ModulePath,
}

/// Build the ADR's shape: a re-exported submodule, one direct consumer, one
/// indirect consumer, and a string literal spelling the module path.
pub fn fixture() -> Fixture {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    std::fs::write(
        root.join("main.tg"),
        "mod driver;\nmod near;\nmod far;\nmod stray;\n",
    )
    .unwrap();

    let driver = root.join("driver");
    std::fs::create_dir(&driver).unwrap();
    std::fs::write(driver.join("mod.tg"), "mod ffi;\npub use driver::ffi::*;\n").unwrap();

    let ffi = driver.join("ffi");
    std::fs::create_dir(&ffi).unwrap();
    // `FfiLocal` is `driver::ffi`'s own item, not the target's: a site importing
    // it goes through a re-export hop and still reaches nothing in `types`.
    std::fs::write(
        ffi.join("mod.tg"),
        "mod types;\npub use driver::ffi::types::*;\n\npub type FfiLocal = { tag: Nat }\n",
    )
    .unwrap();
    std::fs::write(ffi.join("types.tg"), "pub type FfiResult = { code: Nat }\n").unwrap();

    // Names the submodule in its path: a move changes this line.
    std::fs::write(
        root.join("near.tg"),
        "use driver::ffi::types::FfiResult;\n\npub fn near_use(r: FfiResult) -> Nat { r.code }\n",
    )
    .unwrap();

    // Rides the re-export, and carries the harness's generated-source shape.
    std::fs::write(
        root.join("far.tg"),
        "use driver::ffi::FfiResult;\n\n\
         pub fn far_use(r: FfiResult) -> Nat { r.code }\n\n\
         pub fn prelude() -> String {\n    \
         \"    driver::ffi::types::emit(s)\\n\"\n}\n",
    )
    .unwrap();

    // Two sites that must land in neither column: one whose prefix names no
    // module at all (the census's `unresolved`), and one that reaches a hop
    // module for an item the target does not export.
    std::fs::write(
        root.join("stray.tg"),
        "use nowhere::deep::Absent;\nuse driver::ffi::FfiLocal;\n\n\
         pub fn stray_use(t: FfiLocal) -> Nat { t.tag }\n",
    )
    .unwrap();

    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let tree = parse_module_tree(&root.join("main.tg"), &mut visited, &mut chain, None).unwrap();
    let info = build_module_info(&tree);
    let target = ModulePath::root()
        .child("driver".to_string())
        .child("ffi".to_string())
        .child("types".to_string());
    Fixture {
        _dir: dir,
        tree,
        info,
        target,
    }
}

pub fn report_for_fixture() -> DependentsReport {
    let fx = fixture();
    let sources = read_sources(&fx.tree);
    build_report(&fx.tree, &fx.info, &fx.target, &sources)
}

/// 5.9.26f AC4 — a `.tg` string literal holding the path is reported under
/// `literals` and never under `direct`.
#[test]
fn test_string_literal_is_reported_as_a_literal() {
    let report = report_for_fixture();

    assert_eq!(
        report.literals.len(),
        1,
        "expected the generated-source literal, got {:?}",
        report.literals
    );
    let lit = &report.literals[0];
    assert!(lit.text.contains("driver::ffi::types::emit"), "{lit:?}");
    assert!(
        lit.file.ends_with("far.tg"),
        "expected far.tg, got {}",
        lit.file.display()
    );
    assert!(
        !report
            .direct
            .iter()
            .any(|d| d.text.contains("driver::ffi::types::emit")),
        "a text match must never be reported as a resolved reference"
    );
}

/// 5.9.26f AC3 — the census counts every use reference the walk examined, so
/// the reach line is a statement about coverage and not about the finding.
#[test]
fn test_reach_counts_every_use_reference() {
    let fx = fixture();
    let sources = read_sources(&fx.tree);
    let mut sites = Vec::new();
    collect_use_sites(&fx.tree, &ModulePath::root(), &sources, &mut sites);
    assert_eq!(
        sites.len(),
        6,
        "two globs and four named imports: {sites:?}"
    );

    let classified = classify(&sites, &fx.info, &fx.target);
    assert_eq!(
        classified.unresolved, 1,
        "`nowhere::deep` names no module in the tree"
    );
    assert_eq!(classified.resolved, 5);

    let report = build_report(&fx.tree, &fx.info, &fx.target, &sources);
    assert_eq!(
        report.sites_examined, 6,
        "the census is resolved PLUS unresolved — a reference the resolver \
         could not place was still examined, and subtracting it would report \
         less reach than the walk had"
    );
    assert_eq!(report.sites_unresolved, 1);
    assert!(report.modules_in_tree >= 5, "{}", report.modules_in_tree);
}

/// 5.9.26f D4 — a module the tree does not hold is refused, with the near
/// misses that make the refusal actionable.
#[test]
fn test_an_unknown_module_is_refused_with_suggestions() {
    let fx = fixture();

    let found = resolve_target(&fx.info, "driver::ffi::types")
        .expect("a module the tree holds resolves to its path");
    assert_eq!(found, fx.target);

    let missed = resolve_target(&fx.info, "driver::codegen::types")
        .expect_err("no such module — the leaf matches, the parent does not");
    assert!(missed.contains("not found in module tree"), "{missed}");
    assert!(missed.contains("did you mean:"), "{missed}");
    assert!(
        missed.contains("driver::ffi::types"),
        "the suggestion is the module whose LAST segment matches: {missed}"
    );
    assert!(
        !missed.contains("driver::ffi::FfiLocal"),
        "and nothing else: {missed}"
    );

    let silent =
        resolve_target(&fx.info, "no::such::leaf").expect_err("nothing in the tree ends in `leaf`");
    assert!(
        !silent.contains("did you mean"),
        "an empty suggestion block is worse than none: {silent}"
    );
}

/// 5.9.26f D4 — the command refuses a module it cannot resolve, and says so
/// through its exit code rather than only in prose.
#[test]
fn test_the_command_fails_on_an_unknown_module() {
    let fx = fixture();
    let entry = fx.tree.path.clone();
    assert_eq!(
        cmd_info_module_dependents("no::such::module", &entry, false, 1),
        ExitCode::FAILURE,
        "a mis-typed module must not exit 0 over an empty report"
    );
}
