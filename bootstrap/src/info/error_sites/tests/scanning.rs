//! Site discovery, the rendered report, and the I/O wrapper — the half of
//! `info error-sites` that walks source rather than parsing a table.

use super::{CODES, CTORS};
use crate::info::error_sites::*;

// ─────────────────────────────────────────────────────────────────────
// is_plumbing
// ─────────────────────────────────────────────────────────────────────

/// The table and the constructors mention every variant without raising any,
/// and a test fixture is not a place the compiler reports an error. Listing
/// either would bury the handful of real sites.
#[test]
fn plumbing_and_tests_are_not_raise_sites() {
    assert!(is_plumbing("src/elaborate/error/kind/codes.rs"));
    assert!(is_plumbing("src/elaborate/error/constructors.rs"));
    assert!(is_plumbing("src/info/error_sites/tests.rs"));
    assert!(is_plumbing("src/elaborate/tests/items/mod.rs"));
    assert!(is_plumbing("src/foo/bar_tests.rs"));
    assert!(
        is_plumbing("src/info/error_sites/mod.rs"),
        "this tool's own docs"
    );
    assert!(!is_plumbing("src/elaborate/exprs/application.rs"));
}

// ─────────────────────────────────────────────────────────────────────
// is_not_a_construction — patterns vs constructions
// ─────────────────────────────────────────────────────────────────────

/// A `match` arm mentions the variant exactly as a construction does. Missing
/// these made the first run of this tool report four renderers alongside the
/// two real raises for E0013.
#[test]
fn match_arms_and_comments_are_not_constructions() {
    assert!(is_not_a_construction(
        "    ElabErrorKind::Foo(_) => x,",
        None
    ));
    assert!(is_not_a_construction("    /// ElabError::foo(..)", None));
    assert!(is_not_a_construction("    // ElabErrorKind::Foo", None));
    assert!(
        is_not_a_construction("    | ElabErrorKind::Foo(_)", None),
        "or-branch"
    );
}

/// The `=>` of a multi-line or-pattern lands lines below its first variant,
/// so the lookahead is what classifies the head of the arm.
#[test]
fn a_multi_line_or_pattern_is_caught_by_lookahead() {
    let head = "            ElabErrorKind::TypeMismatch { .. }";
    let next = "                | ElabErrorKind::ExpectedFunction(_)";
    assert!(
        is_not_a_construction(head, Some(next)),
        "the head of an or-pattern carries no `=>` of its own"
    );
    assert!(
        !is_not_a_construction(head, Some("            let x = 1;")),
        "and must stay a construction when nothing continues it"
    );
}

/// A real construction survives every filter above.
#[test]
fn a_construction_is_not_filtered() {
    assert!(!is_not_a_construction(
        "        return Err(ElabError::expected_function(sp, ty));",
        Some("    }")
    ));
}

// ─────────────────────────────────────────────────────────────────────
// find_raise_sites
// ─────────────────────────────────────────────────────────────────────

fn app_source() -> (String, String) {
    (
        "src/elaborate/exprs/application.rs".to_string(),
        r#"
impl Elaborator {
    fn apply_args_sequentially(&mut self) -> Result<()> {
        let Type::Arrow(a, b) = ty else {
            return Err(ElabError::expected_function(arg.span(), ty));
        };
        Ok(())
    }

    fn unrelated(&self) {}
}
"#
        .to_string(),
    )
}

/// The site is attributed to the enclosing function — which is the answer
/// the question is really after ("which boundary raises this?").
#[test]
fn a_constructor_call_is_found_and_attributed_to_its_function() {
    let sites = find_raise_sites(
        "ExpectedFunction",
        &["expected_function".to_string()],
        &[app_source()],
    );
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(
        sites[0].function.as_deref(),
        Some("apply_args_sequentially")
    );
    assert_eq!(sites[0].via, RaiseVia::Constructor);
    assert_eq!(sites[0].file, "src/elaborate/exprs/application.rs");
}

/// A variant built inline, with no named constructor, is still a raise site.
/// The `via` label is user-visible text, so assert it rather than only the
/// enum: `[variant]` vs `[constructor]` is how a reader tells an inline
/// construction from a call to a named constructor.
#[test]
fn the_report_labels_a_variant_construction_as_such() {
    let files = [(
        "src/elaborate/items/mod.rs".to_string(),
        "fn collect() { record(ElabError::new(s, ElabErrorKind::ExpectedFunction(t))); }"
            .to_string(),
    )];
    let sites = find_raise_sites("ExpectedFunction", &[], &files);
    let out = render_report("ExpectedFunction", "E0013", &[], &sites);
    assert!(out.contains("[variant]"), "{out}");
    assert!(!out.contains("[constructor]"), "{out}");
}

#[test]
fn a_direct_variant_construction_is_found() {
    let files = [(
        "src/elaborate/items/mod.rs".to_string(),
        "fn collect() { record(ElabError::new(s, ElabErrorKind::ExpectedFunction(t))); }"
            .to_string(),
    )];
    let sites = find_raise_sites("ExpectedFunction", &[], &files);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].via, RaiseVia::Variant);
}

/// A different kind's constructor must not be attributed to this one — the
/// scan keys on the resolved constructor set, not on "looks like an error".
#[test]
fn another_kinds_sites_are_not_reported() {
    let files = [(
        "src/elaborate/exprs/paths.rs".to_string(),
        "fn f() { Err(ElabError::undefined_variable(span, name)) }".to_string(),
    )];
    assert!(find_raise_sites("ExpectedFunction", &["expected_function".into()], &files).is_empty());
}

/// A `#[cfg(test)]` module inside a production file is test code too — its
/// fixtures must not be attributed to the compiler.
#[test]
fn a_test_module_inside_a_production_file_is_skipped() {
    let files = [(
        "src/elaborate/exprs/paths.rs".to_string(),
        "fn real() { ElabError::expected_function(a, b); }\n\
         #[cfg(test)]\n\
         mod tests {\n\
         fn fixture() { ElabError::expected_function(c, d); }\n\
         }"
        .to_string(),
    )];
    let sites = find_raise_sites("ExpectedFunction", &["expected_function".into()], &files);
    assert_eq!(sites.len(), 1, "only the production raise: {sites:?}");
    assert_eq!(sites[0].function.as_deref(), Some("real"));
}

#[test]
fn sites_are_ordered_by_file_then_line() {
    let files = [
        (
            "src/z_later.rs".to_string(),
            "fn z() { ElabError::expected_function(a, b); }".to_string(),
        ),
        (
            "src/a_first.rs".to_string(),
            "\n\nfn a() { ElabError::expected_function(a, b); }".to_string(),
        ),
    ];
    let sites = find_raise_sites("ExpectedFunction", &["expected_function".into()], &files);
    assert_eq!(
        sites.iter().map(|s| s.file.as_str()).collect::<Vec<_>>(),
        vec!["src/a_first.rs", "src/z_later.rs"],
        "walk order must not leak into the report"
    );
    assert_eq!(sites[0].line, 3, "line is 1-based");
}

// ─────────────────────────────────────────────────────────────────────
// render_report
// ─────────────────────────────────────────────────────────────────────

#[test]
fn the_report_names_the_kind_constructor_and_sites() {
    let sites = find_raise_sites(
        "ExpectedFunction",
        &["expected_function".to_string()],
        &[app_source()],
    );
    let out = render_report(
        "ExpectedFunction",
        "E0013",
        &["expected_function".into()],
        &sites,
    );
    assert!(
        out.contains("E0013 — ElabErrorKind::ExpectedFunction"),
        "{out}"
    );
    assert!(out.contains("ElabError::expected_function"), "{out}");
    assert!(out.contains("apply_args_sequentially()"), "{out}");
    assert!(out.contains("raise site(s): 1"), "{out}");
    assert!(
        out.contains("[constructor]"),
        "the report must say HOW the kind was built — a site reached through a \
         named constructor reads differently from an inline variant: {out}"
    );
}

/// Zero sites must SAY zero — an empty list rendering as a bare header is
/// how "found nothing" and "looked at nothing" become indistinguishable.
#[test]
fn no_sites_says_so_explicitly() {
    let out = render_report("CannotInferType", "E0011", &[], &[]);
    assert!(out.contains("no raise sites found"), "{out}");
    assert!(out.contains("constructed directly"), "{out}");
}

// ─────────────────────────────────────────────────────────────────────
// Against the real table (the oracle this tool trusts)
// ─────────────────────────────────────────────────────────────────────

/// The live `codes.rs` must parse and resolve — if its shape ever changes,
/// this fails here rather than the tool silently reporting every code
/// unknown.
#[test]
fn the_live_table_resolves_a_known_code() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/elaborate/error/kind/codes.rs"
    );
    let src = std::fs::read_to_string(path).expect("the code table must be readable");
    let pairs = parse_kind_codes(&src);
    assert!(pairs.len() >= 50, "parsed only {} arm(s)", pairs.len());
    assert_eq!(
        resolve_query("E0013", &pairs),
        Some(("ExpectedFunction".to_string(), "E0013".to_string()))
    );
}

// ─────────────────────────────────────────────────────────────────────
// The I/O wrapper — the half a pure-function seam cannot reach
// ─────────────────────────────────────────────────────────────────────
//
// Every test above passes source in as a string, so none of them exercises
// what the binary actually does when a user types the command: locate the
// tree, walk it, resolve against the REAL table. That gap is how a tool ships
// with a green suite and a default that points nowhere.

#[test]
fn the_walk_reads_rs_files_and_shows_paths_relative_to_the_base() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    std::fs::create_dir_all(base.join("nested")).unwrap();
    std::fs::write(base.join("b_second.rs"), "fn b() {}").unwrap();
    std::fs::write(base.join("a_first.rs"), "fn a() {}").unwrap();
    std::fs::write(base.join("nested/deep.rs"), "fn d() {}").unwrap();
    std::fs::write(base.join("ignored.txt"), "not rust").unwrap();

    let found = read_rust_sources(base, base);
    let names: Vec<&str> = found.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        !names.iter().any(|n| n.ends_with(".txt")),
        "only .rs files: {names:?}"
    );
    assert_eq!(found.len(), 3, "{names:?}");
    assert!(
        names.contains(&"nested/deep.rs") || names.contains(&"nested\\deep.rs"),
        "the walk must recurse and show a relative path: {names:?}"
    );
    assert!(
        found.iter().any(|(_, src)| src.contains("fn a()")),
        "contents come back with the path"
    );
}

#[test]
fn the_walk_of_a_missing_directory_is_empty_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-dir");
    assert!(read_rust_sources(&missing, dir.path()).is_empty());
}

/// The command end to end, against the real source tree: a known code
/// succeeds and an unknown one fails. This is the only test that would catch
/// the table moving out from under the hard-coded path in `run`.
#[test]
fn the_command_succeeds_on_a_real_code_and_fails_on_a_bogus_one() {
    assert_eq!(
        format!("{:?}", run("E0013")),
        format!("{:?}", ExitCode::SUCCESS),
        "a real code must resolve against the live table"
    );
    assert_eq!(
        format!("{:?}", run("E4242")),
        format!("{:?}", ExitCode::FAILURE),
        "an unknown code must fail, not print an empty report"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Guards that only a discriminating fixture can pin
// ─────────────────────────────────────────────────────────────────────

/// The code filter is `length == 5` AND `starts with E/W`. A table of
/// well-formed codes cannot tell that from `OR` — this one can.
#[test]
fn a_malformed_code_is_rejected_on_both_halves_of_the_guard() {
    let table = r#"
    ElabErrorKind::Good(_) => "E0001",
    ElabErrorKind::WrongPrefix(_) => "X0002",
    ElabErrorKind::TooShort(_) => "E003",
"#;
    let pairs = parse_kind_codes(table);
    let kinds: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        kinds,
        vec!["Good"],
        "a 5-char non-E/W code and a 4-char E code must both be rejected"
    );
}

/// The lookahead is `lines[idx + 1]`. Driven through `find_raise_sites` — the
/// pure-function test of `is_not_a_construction` passes `next` by hand and so
/// never exercises the indexing.
#[test]
fn the_lookahead_reads_the_next_line_through_the_scan() {
    let files = [(
        "src/driver/hints.rs".to_string(),
        "fn from_kind(k: &Kind) -> C {\n\
         match k {\n\
         ElabErrorKind::ExpectedFunction(_)\n\
         | ElabErrorKind::CannotInferType => C::Type,\n\
         }\n\
         }"
        .to_string(),
    )];
    assert!(
        find_raise_sites("ExpectedFunction", &[], &files).is_empty(),
        "the head of an or-pattern must be skipped via the next line, not counted"
    );
}

/// Line numbers are 1-based: `idx + 1`, not `idx`.
#[test]
fn the_reported_line_is_one_based_through_the_scan() {
    let files = [(
        "src/a.rs".to_string(),
        "fn f() {\n    ElabError::expected_function(a, b);\n}".to_string(),
    )];
    let sites = find_raise_sites("ExpectedFunction", &["expected_function".into()], &files);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].line, 2, "the call is on the second line");
}

/// The constructor selection must match the resolved kind, not merely differ
/// from it — an inverted comparison reports every OTHER kind's constructors,
/// which an exit code cannot see.
#[test]
fn only_the_resolved_kinds_constructors_are_selected() {
    let parsed = parse_constructors(CTORS);
    assert_eq!(
        constructors_for_kind(&parsed, "ExpectedFunction"),
        vec!["expected_function".to_string()]
    );
    assert!(
        constructors_for_kind(&parsed, "NoSuchKind").is_empty(),
        "a kind with no constructor selects none, not all"
    );
}
