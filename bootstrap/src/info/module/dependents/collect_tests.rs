//! Tests for the inversion in [`super::collect`] (ADR 5.9.26f).
//!
//! The direct/indirect split (D2), the re-export fixpoint, the ordering the
//! report presents, and the string-literal scan (D3) — every one of them pure
//! over the shared fixture or over an injected source string.

use std::path::PathBuf;

use tungsten_bootstrap::ast::Path as AstPath;
use tungsten_bootstrap::elaborate::ModulePath;

use super::collect::{line_of, reexporters_of, split_path};
use super::tests::{fixture, report_for_fixture};
use super::{classify, collect_use_sites, find_literal_refs, read_sources, Dependent, UseSite};

/// 5.9.26f AC1 — the site naming the submodule is direct, the site importing
/// through the parent's re-export is indirect.
#[test]
fn test_direct_and_indirect_are_split() {
    let report = report_for_fixture();

    let direct: Vec<&str> = report.direct.iter().map(|d| d.text.as_str()).collect();
    let indirect: Vec<&str> = report.indirect.iter().map(|d| d.text.as_str()).collect();

    // Both sites that spell `driver::ffi::types` are direct — the consumer's
    // and the parent's own re-export, which is what publishes the items.
    assert!(
        direct.contains(&"use driver::ffi::types::FfiResult"),
        "the submodule-naming consumer is direct, got {direct:?}"
    );
    assert!(
        direct.contains(&"pub use driver::ffi::types::*"),
        "the re-export that names the submodule is direct too, got {direct:?}"
    );
    assert!(
        indirect.contains(&"use driver::ffi::FfiResult"),
        "the re-export rider is indirect, got {indirect:?}"
    );
    assert!(
        !direct.contains(&"use driver::ffi::FfiResult"),
        "a re-export rider must never be reported direct"
    );
    assert!(
        !indirect.contains(&"use driver::ffi::types::FfiResult"),
        "a site naming the submodule must never be reported indirect"
    );
}

/// 5.9.26f AC1 — each direct site carries the file and line a regroup edits.
#[test]
fn test_direct_site_is_located() {
    let report = report_for_fixture();
    let site = report
        .direct
        .iter()
        .find(|d| d.file.ends_with("near.tg"))
        .expect("the near.tg consumer is a direct dependent");
    assert_eq!(site.line, 1, "the use is on line 1 of near.tg");
    assert_eq!(site.via, None, "a direct site reaches the module by no hop");
}

/// 5.9.26f AC1 — the fixpoint walks the whole `pub use` chain, so `driver`
/// counts as a hop even though it re-exports `driver::ffi`, not the target.
#[test]
fn test_reexport_hops_are_transitive() {
    let fx = fixture();
    let sources = read_sources(&fx.tree);
    let mut sites = Vec::new();
    collect_use_sites(&fx.tree, &ModulePath::root(), &sources, &mut sites);

    let hops = reexporters_of(&sites, &fx.info, &fx.target);
    let ffi = ModulePath::root()
        .child("driver".to_string())
        .child("ffi".to_string());
    assert!(hops.contains(&ffi), "driver::ffi re-exports the target");
    assert!(
        hops.contains(&ModulePath::root().child("driver".to_string())),
        "driver re-exports driver::ffi, so it is a hop too: {hops:?}"
    );
}

/// 5.9.26f AC1 — a site reaching a re-export hop for an item the target does
/// not export is neither direct nor indirect.
///
/// The hop test and the item test are an `&&` for this case alone: `driver::ffi`
/// is a genuine re-exporter of `driver::ffi::types`, so the hop half passes, and
/// only `FfiLocal` not being one of the target's items keeps the site out of the
/// regroup worklist. Either half alone reports a dependency that is not one.
#[test]
fn test_a_hop_import_of_a_foreign_item_is_not_a_dependent() {
    let report = report_for_fixture();
    let named = |deps: &[Dependent]| {
        deps.iter()
            .any(|dep| dep.text.contains("driver::ffi::FfiLocal"))
    };
    assert!(
        !named(&report.direct),
        "it does not name the target: {:?}",
        report.direct
    );
    assert!(
        !named(&report.indirect),
        "it rides a hop, but takes an item the target does not export — a move \
         of `types` does not touch it: {:?}",
        report.indirect
    );
}

/// 5.9.26f AC1 — the two columns are ordered by the location a regroup edits,
/// not by the order the tree walk happened to reach them.
#[test]
fn test_dependents_are_ordered_by_location() {
    let fx = fixture();
    let site = |file: &str, line: usize| UseSite {
        module: ModulePath::root().child("near".to_string()),
        file: PathBuf::from(file),
        line,
        prefix: vec!["driver".into(), "ffi".into(), "types".into()],
        item: Some("FfiResult".into()),
        is_pub: false,
        text: format!("use driver::ffi::types::FfiResult // {file}:{line}"),
    };

    // Deliberately out of order on all three key components.
    let sites = vec![site("z.tg", 9), site("a.tg", 7), site("a.tg", 2)];
    let classified = classify(&sites, &fx.info, &fx.target);

    let located: Vec<(String, usize)> = classified
        .direct
        .iter()
        .map(|dep| (dep.file.display().to_string(), dep.line))
        .collect();
    assert_eq!(
        located,
        vec![
            ("a.tg".to_string(), 2),
            ("a.tg".to_string(), 7),
            ("z.tg".to_string(), 9),
        ],
        "file, then line: a regroup works down a file, so an unsorted list \
         costs the reader the one grouping the answer is used in"
    );
}

/// 5.9.26f AC1 — a single-segment path names no module, so it never enters the
/// census; a two-segment one does.
#[test]
fn test_split_path_needs_a_module_prefix() {
    let span = tungsten_bootstrap::span::Span::new(0, 0);
    let path = |names: &[&str]| AstPath {
        segments: names
            .iter()
            .map(|name| tungsten_bootstrap::ast::Ident::new(*name, span))
            .collect(),
        span,
    };

    assert_eq!(
        split_path(&path(&["FfiResult"])),
        None,
        "an unqualified name carries no module prefix to classify"
    );
    assert_eq!(
        split_path(&path(&["near", "near_use"])),
        Some((vec!["near".to_string()], Some("near_use".to_string()))),
        "the shortest qualified path is still a module reference, and dropping \
         it would silently shrink the reach line"
    );
    assert_eq!(
        split_path(&path(&["driver", "ffi", "types", "FfiResult"])),
        Some((
            vec!["driver".to_string(), "ffi".to_string(), "types".to_string()],
            Some("FfiResult".to_string())
        ))
    );
}

/// 5.9.26f AC4 — the literal scanner reads literals, not code, and not a
/// `use` line that happens to name the same path.
#[test]
fn test_literal_scan_ignores_code() {
    let src = "use driver::ffi::types::X;\nfn f() -> String { \"driver::ffi::types::y\" }\n";
    let hits = find_literal_refs(src, "driver::ffi::types");
    assert_eq!(hits.len(), 1, "only the literal matches: {hits:?}");
    assert_eq!(hits[0].0, 2, "1-based line of the literal");
    assert_eq!(hits[0].1, "driver::ffi::types::y");
}

/// 5.9.26f AC4 — code sharing a line with a literal is not part of the literal.
#[test]
fn test_literal_scan_ignores_code_beside_a_literal() {
    let src = "use driver::ffi::types::X; fn f() -> String { \"hello\" }\n";
    assert!(
        find_literal_refs(src, "driver::ffi::types").is_empty(),
        "the only literal on the line is \"hello\"; reading the code before it \
         as literal text would report the `use` twice — once resolved and once \
         as a text match"
    );
}

/// 5.9.26f AC4 — an escaped quote inside a literal does not end it, so the
/// harness's `\n`-terminated generated lines scan as one literal.
#[test]
fn test_literal_scan_handles_escapes() {
    let src = "fn f() -> String { \"a\\\"b driver::ffi::types\\n\" }\n";
    let hits = find_literal_refs(src, "driver::ffi::types");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].1.starts_with("a\\\"b "), "{:?}", hits[0].1);
}

/// 5.9.26f AC1 — a site's line is the line a regroup edits, so the offset →
/// line map has to hold at both ends of a file.
#[test]
fn test_line_of_counts_newlines() {
    assert_eq!(line_of("a\nb\nc", 0), 1);
    assert_eq!(line_of("a\nb\nc", 2), 2);
    assert_eq!(line_of("a\nb\nc", 4), 3);
    assert_eq!(line_of("a\nb\nc", 999), 3, "an out-of-range offset clamps");
}
