//! Tests for the per-module normalization oracle's harvest helpers
//! (ADR 22.7.26b). The end-to-end oracle behaviour (fresh-vs-stored
//! classification, tier-2 normalization) is covered in
//! `doctor/checks/type_checks/check_normalization/tests.rs`.

use super::make_parsed_module;
use crate::driver::per_module::fresh_encodings::{collect_modules_post_order, own_type_names};

/// `own_type_names` picks up ADT definitions and aliases, and ignores
/// functions/imports — the module's harvestable key set.
#[test]
fn own_type_names_lists_type_defs_and_aliases_only() {
    let (ast, errors) =
        crate::parse("type Color = Red | Green\ntype Meters = Nat\nfn f() -> Nat { 0 }\n");
    assert!(errors.is_empty(), "fixture must parse cleanly: {errors:?}");
    assert_eq!(own_type_names(&ast), vec!["Color", "Meters"]);
}

#[test]
fn own_type_names_empty_for_value_only_module() {
    let (ast, errors) = crate::parse("fn f() -> Nat { 0 }\n");
    assert!(errors.is_empty(), "fixture must parse cleanly: {errors:?}");
    assert!(own_type_names(&ast).is_empty());
}

/// The walk is post-order (children before parents), matching Body Elaboration's
/// elaboration order so first-harvest-wins mirrors first-elaborated-wins.
#[test]
fn collect_modules_post_order_visits_children_before_parents() {
    let leaf_a = make_parsed_module(vec![]);
    let leaf_b = make_parsed_module(vec![]);
    let mut root = make_parsed_module(vec![]);
    root.submodules = vec![leaf_a, leaf_b];

    let mut visited = Vec::new();
    collect_modules_post_order(&root, &mut visited);

    assert_eq!(visited.len(), 3);
    // The root is visited last; its two children come first in order.
    assert!(std::ptr::eq(visited[0], &root.submodules[0]));
    assert!(std::ptr::eq(visited[1], &root.submodules[1]));
    assert!(std::ptr::eq(visited[2], &root));
}
