use super::*;

/// Build an adjacency from `(name, callees)` pairs. Every name given as a key
/// becomes a definition; names appearing only as callees do not.
fn graph(edges: &[(&str, &[&str])]) -> Adjacency {
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

fn names(set: &BTreeSet<String>) -> Vec<&str> {
    set.iter().map(String::as_str).collect()
}

fn roots(of: &[&str]) -> BTreeSet<String> {
    of.iter().map(|r| (*r).to_string()).collect()
}

// ---------------------------------------------------------------------------
// callers_of
// ---------------------------------------------------------------------------

#[test]
fn callers_of_finds_every_referencing_definition() {
    let g = graph(&[
        ("main", &["helper"]),
        ("other", &["helper"]),
        ("helper", &[]),
    ]);
    assert_eq!(names(&callers_of(&g, "helper")), vec!["main", "other"]);
}

#[test]
fn a_definition_nothing_references_has_no_callers() {
    let g = graph(&[("main", &[]), ("orphan", &[])]);
    assert!(callers_of(&g, "orphan").is_empty());
}

#[test]
fn self_recursion_counts_as_a_caller() {
    // The distinction this preserves: "recursive but unreachable" still needs an
    // entry point, and reporting zero callers for it would read as "not
    // recursive".
    let g = graph(&[("spin", &["spin"])]);
    assert_eq!(names(&callers_of(&g, "spin")), vec!["spin"]);
}

#[test]
fn callers_of_a_name_that_is_not_a_definition_still_reports_its_referrers() {
    // An extern has no key of its own but is still referenced; asking who calls
    // it is a legitimate question.
    let g = graph(&[("main", &["tg_print"])]);
    assert_eq!(names(&callers_of(&g, "tg_print")), vec!["main"]);
}

#[test]
fn callers_of_an_absent_name_is_empty_rather_than_a_panic() {
    let g = graph(&[("main", &[])]);
    assert!(callers_of(&g, "nonexistent").is_empty());
}

// ---------------------------------------------------------------------------
// invert
// ---------------------------------------------------------------------------

#[test]
fn invert_gives_every_definition_a_row_including_the_uncalled() {
    let g = graph(&[("main", &["helper"]), ("helper", &[]), ("orphan", &[])]);
    let inverted = invert(&g);

    assert_eq!(inverted.len(), 3, "every definition needs a row");
    assert_eq!(names(&inverted["helper"]), vec!["main"]);
    assert!(
        inverted["orphan"].is_empty(),
        "uncalled must be empty, not absent"
    );
    assert!(inverted["main"].is_empty());
}

#[test]
fn invert_gives_no_row_to_a_name_that_is_not_a_definition() {
    // Otherwise every extern and builtin shows up in a dead-code census.
    let g = graph(&[("main", &["tg_print"])]);
    let inverted = invert(&g);
    assert!(!inverted.contains_key("tg_print"));
    assert_eq!(inverted.len(), 1);
}

#[test]
fn invert_agrees_with_callers_of_on_every_definition() {
    let g = graph(&[
        ("main", &["a", "b"]),
        ("a", &["b"]),
        ("b", &["b"]),
        ("dead", &["a"]),
    ]);
    let inverted = invert(&g);
    for name in g.keys() {
        assert_eq!(
            inverted[name],
            callers_of(&g, name),
            "the two spellings disagree about {name}"
        );
    }
}

// ---------------------------------------------------------------------------
// reachable_from
// ---------------------------------------------------------------------------

#[test]
fn reachability_follows_the_call_chain_transitively() {
    let g = graph(&[("main", &["a"]), ("a", &["b"]), ("b", &[]), ("dead", &[])]);
    assert_eq!(
        names(&reachable_from(&g, &roots(&["main"]))),
        vec!["a", "b", "main"]
    );
}

#[test]
fn a_cycle_terminates_rather_than_looping() {
    // Every real call graph here has cycles; without the seen-set this hangs.
    let g = graph(&[("main", &["a"]), ("a", &["b"]), ("b", &["a"])]);
    assert_eq!(
        names(&reachable_from(&g, &roots(&["main"]))),
        vec!["a", "b", "main"]
    );
}

#[test]
fn a_root_that_is_not_a_definition_contributes_nothing() {
    let g = graph(&[("main", &[])]);
    assert_eq!(
        names(&reachable_from(&g, &roots(&["main", "absent"]))),
        vec!["main"]
    );
}

#[test]
fn no_roots_reaches_nothing() {
    let g = graph(&[("main", &["a"]), ("a", &[])]);
    assert!(reachable_from(&g, &BTreeSet::new()).is_empty());
}

#[test]
fn several_roots_union_their_reach() {
    let g = graph(&[
        ("main", &["a"]),
        ("test_one", &["b"]),
        ("a", &[]),
        ("b", &[]),
        ("dead", &[]),
    ]);
    let reached = reachable_from(&g, &roots(&["main", "test_one"]));
    assert_eq!(names(&reached), vec!["a", "b", "main", "test_one"]);
}

#[test]
fn reachability_does_not_walk_backwards() {
    // `caller` reaches `callee`, never the reverse.
    let g = graph(&[("caller", &["callee"]), ("callee", &[])]);
    assert_eq!(
        names(&reachable_from(&g, &roots(&["callee"]))),
        vec!["callee"]
    );
}

// ---------------------------------------------------------------------------
// unreachable_from
// ---------------------------------------------------------------------------

#[test]
fn unreachable_is_the_complement_over_definitions() {
    let g = graph(&[("main", &["a"]), ("a", &[]), ("dead", &[])]);
    assert_eq!(
        names(&unreachable_from(&g, &roots(&["main"]))),
        vec!["dead"]
    );
}

#[test]
fn a_mutually_recursive_island_is_unreachable_despite_having_callers() {
    // The case a caller-count census cannot see, and the reason this function
    // exists rather than "definitions whose inverted row is empty".
    let g = graph(&[("main", &[]), ("ping", &["pong"]), ("pong", &["ping"])]);

    let dead = unreachable_from(&g, &roots(&["main"]));
    assert_eq!(names(&dead), vec!["ping", "pong"]);
    // Both have a caller, so counting callers would have called them live.
    assert!(!callers_of(&g, "ping").is_empty());
    assert!(!callers_of(&g, "pong").is_empty());
}

#[test]
fn everything_reachable_leaves_nothing_unreachable() {
    let g = graph(&[("main", &["a"]), ("a", &["b"]), ("b", &[])]);
    assert!(unreachable_from(&g, &roots(&["main"])).is_empty());
}

#[test]
fn with_no_roots_every_definition_is_unreachable() {
    let g = graph(&[("main", &["a"]), ("a", &[])]);
    assert_eq!(
        names(&unreachable_from(&g, &BTreeSet::new())),
        vec!["a", "main"]
    );
}

#[test]
fn a_self_recursive_orphan_is_unreachable() {
    // Self-recursion gives it a caller; it is still dead.
    let g = graph(&[("main", &[]), ("spin", &["spin"])]);
    assert_eq!(
        names(&unreachable_from(&g, &roots(&["main"]))),
        vec!["spin"]
    );
}
