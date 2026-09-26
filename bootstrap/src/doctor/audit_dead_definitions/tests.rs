//! Root-policy and rendering tests over injected data, plus a fixture pair that
//! drives the command end to end.
//!
//! `TempDir` throughout, deliberately: it releases on drop, and ADR 18.8.26d
//! exists because helpers that do not are how this machine accumulated 26,026
//! stranded directories.

use std::fs;
use std::process::ExitCode;

use tempfile::TempDir;

use super::*;

fn set(of: &[&str]) -> BTreeSet<String> {
    of.iter().map(|n| (*n).to_string()).collect()
}

// ---------------------------------------------------------------------------
// The root policy
// ---------------------------------------------------------------------------

#[test]
fn main_and_test_prefixed_names_are_roots() {
    assert!(is_default_root("main"));
    assert!(is_default_root("test_partial_reaches_the_ast"));
}

#[test]
fn a_name_merely_starting_with_test_is_not_a_root() {
    // `test_` not `test`: admitting `tester` would silently mark everything it
    // reaches as live.
    assert!(!is_default_root("tester"));
    assert!(!is_default_root("testing"));
    assert!(!is_default_root("attest_claim"));
}

#[test]
fn an_ordinary_definition_is_not_a_root() {
    assert!(!is_default_root("helper"));
    assert!(!is_default_root("mainline"));
}

#[test]
fn roots_are_drawn_only_from_definitions_that_exist() {
    let names = set(&["main", "helper", "test_one"]);
    assert_eq!(roots_for(&names, &[]), set(&["main", "test_one"]));
}

#[test]
fn extra_roots_are_added_even_when_absent_from_the_file() {
    // A `--root` naming something not in this file is a policy statement, not
    // an error — the caller may be pointing at an external entry point.
    let names = set(&["helper"]);
    let roots = roots_for(&names, &["api_entry".to_string()]);
    assert_eq!(roots, set(&["api_entry"]));
}

#[test]
fn extra_roots_compose_with_the_defaults_rather_than_replacing_them() {
    let names = set(&["main", "helper"]);
    let roots = roots_for(&names, &["helper".to_string()]);
    assert_eq!(roots, set(&["main", "helper"]));
}

#[test]
fn a_file_with_no_entry_point_yields_no_roots() {
    assert!(roots_for(&set(&["a", "b"]), &[]).is_empty());
}

// ---------------------------------------------------------------------------
// The census, end to end over an adjacency
// ---------------------------------------------------------------------------

fn graph(edges: &[(&str, &[&str])]) -> tungsten_core::terms::termination::Adjacency {
    edges
        .iter()
        .map(|(name, callees)| {
            (
                (*name).to_string(),
                callees.iter().map(|c| (*c).to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn the_census_finds_what_no_root_reaches() {
    let g = graph(&[("main", &["live"]), ("live", &[]), ("dead", &[])]);
    let result = census_of(&g, &[]);

    assert_eq!(result.examined, 3);
    assert_eq!(result.roots, set(&["main"]));
    assert_eq!(result.dead, set(&["dead"]));
}

#[test]
fn a_test_prefixed_definition_is_its_own_entry_point() {
    // The `tungsten test` convention: the runner calls these, no definition does.
    let g = graph(&[("test_one", &["helper"]), ("helper", &[])]);
    let result = census_of(&g, &[]);

    assert_eq!(result.roots, set(&["test_one"]));
    assert!(result.dead.is_empty(), "{:?}", result.dead);
}

#[test]
fn an_extra_root_rescues_the_subtree_below_it() {
    let g = graph(&[("api", &["helper"]), ("helper", &[])]);

    assert_eq!(census_of(&g, &[]).dead, set(&["api", "helper"]));
    assert!(census_of(&g, &["api".to_string()]).dead.is_empty());
}

#[test]
fn an_empty_graph_examines_nothing_and_finds_nothing() {
    let result = census_of(&graph(&[]), &[]);
    assert_eq!(result.examined, 0);
    assert!(result.dead.is_empty());
    assert!(render_census(&result).contains("nothing was examined"));
}

#[test]
fn a_graph_with_no_entry_point_reports_everything_and_says_why() {
    let g = graph(&[("a", &["b"]), ("b", &[])]);
    let result = census_of(&g, &[]);

    assert!(result.roots.is_empty());
    assert_eq!(result.dead, set(&["a", "b"]));
    assert!(render_census(&result).contains("the walk started nowhere"));
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn census(examined: usize, roots: &[&str], dead: &[&str]) -> DeadDefinitionCensus {
    DeadDefinitionCensus {
        examined,
        roots: set(roots),
        dead: set(dead),
    }
}

#[test]
fn the_reach_line_carries_the_denominator() {
    let out = render_census(&census(2207, &["main"], &[]));
    assert!(out.contains("2207 definition(s) examined"), "{out}");
    assert!(out.contains("1 root(s)"), "{out}");
}

#[test]
fn a_clean_census_says_so_explicitly() {
    let out = render_census(&census(3, &["main"], &[]));
    assert!(out.contains("every definition is reachable"), "{out}");
}

#[test]
fn an_empty_examination_is_distinguishable_from_a_clean_one() {
    // The failure this repo keeps rediscovering: `0 dead` over nothing renders
    // like `0 dead` over everything.
    let empty = render_census(&census(0, &["main"], &[]));
    let clean = render_census(&census(3, &["main"], &[]));
    assert!(empty.contains("nothing was examined"), "{empty}");
    assert!(!clean.contains("nothing was examined"), "{clean}");
}

#[test]
fn a_census_with_no_roots_says_the_walk_started_nowhere() {
    // Otherwise every definition reads as dead and the report is confidently
    // wrong rather than visibly unusable.
    let out = render_census(&census(5, &[], &["a", "b"]));
    assert!(out.contains("the walk started nowhere"), "{out}");
}

#[test]
fn the_roots_are_printed_so_a_wrong_policy_is_visible() {
    let out = render_census(&census(5, &["main", "test_x"], &[]));
    assert!(out.contains("roots: main, test_x"), "{out}");
}

#[test]
fn dead_definitions_are_listed_and_counted() {
    let out = render_census(&census(5, &["main"], &["orphan", "ping"]));
    assert!(out.contains("2 unreachable definition(s)"), "{out}");
    assert!(out.contains("  orphan"), "{out}");
    assert!(out.contains("  ping"), "{out}");
}

/// ADR 20.8.26c: a bootstrap-intercepted name is dead HERE and may be live in
/// the self-host, so it is marked in the LIST — a footnote is not enough when
/// the list is what gets skimmed for deletion candidates.
#[test]
fn an_intercepted_builtin_is_marked_in_the_list_and_explained_below() {
    let out = render_census(&census(5, &["main"], &["string_len", "orphan"]));
    assert!(out.contains("string_len   ** intercepted builtin"), "{out}");
    assert!(out.contains("  orphan\n"), "{out}");
    assert!(out.contains("BEFORE name resolution"), "{out}");
    assert!(out.contains("info builtins"), "{out}");
}

/// The complement: an ordinary census carries no interception warning, so the
/// paragraph does not train its reader to skip it.
#[test]
fn a_census_with_no_intercepted_name_carries_no_interception_warning() {
    let out = render_census(&census(5, &["main"], &["orphan", "ping"]));
    assert!(!out.contains("intercepted builtin"), "{out}");
    assert!(!out.contains("info builtins"), "{out}");
}

#[test]
fn a_finding_names_the_follow_up_commands() {
    // The census answers "which"; `info def --callers` answers "why", and a
    // reader who has to guess the next command usually stops here.
    let out = render_census(&census(5, &["main"], &["orphan"]));
    assert!(out.contains("--callers"), "{out}");
    assert!(out.contains("--root"), "{out}");
}

#[test]
fn a_finding_warns_that_the_census_is_per_entry_file() {
    // Measured on `src/compiler/main.tg`: 424 of 2,207 unreachable, and a large
    // share of those are helpers the separate `test_*.tg` entry files call. They
    // are correctly listed and must NOT be deleted — without this line a reader
    // takes the list as a deletion worklist.
    let out = render_census(&census(2207, &["main"], &["import_list_lookup"]));
    assert!(out.contains("PER ENTRY FILE"), "{out}");
}

#[test]
fn a_clean_census_does_not_print_the_follow_up_noise() {
    let out = render_census(&census(5, &["main"], &[]));
    assert!(!out.contains("--callers"), "{out}");
}

// ---------------------------------------------------------------------------
// The command, end to end over a real fixture
// ---------------------------------------------------------------------------

/// Write `source` to a scratch `.tg` file and run the census over it.
fn run_over(source: &str, extra_roots: &[String]) -> (ExitCode, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("fixture.tg");
    fs::write(&path, source).expect("write fixture");
    let code = cmd_audit_dead_definitions(&path, false, 20, extra_roots);
    (code, dir)
}

#[test]
fn the_command_succeeds_on_a_file_with_dead_code() {
    // Reporting, never gating: a finding must not fail the build.
    let (code, _dir) = run_over("fn main() -> Nat { 0 }\nfn orphan() -> Nat { 1 }\n", &[]);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn the_command_succeeds_on_a_clean_file() {
    let (code, _dir) = run_over("fn main() -> Nat { 0 }\n", &[]);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn the_command_accepts_an_extra_root() {
    let (code, _dir) = run_over(
        "fn main() -> Nat { 0 }\nfn api() -> Nat { 1 }\n",
        &["api".to_string()],
    );
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn an_unparseable_file_fails_rather_than_reporting_an_empty_census() {
    // The dangerous direction: a file that could not be elaborated must not
    // render as "0 definitions examined, nothing dead".
    let (code, _dir) = run_over("fn main( -> { \n", &[]);
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn a_missing_file_fails() {
    let dir = TempDir::new().expect("tempdir");
    let missing = dir.path().join("does-not-exist.tg");
    assert_eq!(
        cmd_audit_dead_definitions(&missing, false, 20, &[]),
        ExitCode::FAILURE
    );
}
