//! Tests for the stored-type-tree size walker (ADR 8.7.26a §2.3).

use super::*;
use crate::types::Type;

#[test]
fn lone_leaf_is_one_node_depth_one() {
    let metrics = measure_type(&Type::Nat);
    assert_eq!(metrics.node_count, 1);
    assert_eq!(metrics.max_depth, 1);
    assert!(metrics.mu_binder_chain.is_empty());
    assert!(metrics.alpha_occurrences.is_empty());
}

#[test]
fn arrow_of_leaves_counts_three_nodes_depth_two() {
    let ty = Type::arrow(Type::Nat, Type::Bool);
    let metrics = measure_type(&ty);
    assert_eq!(metrics.node_count, 3);
    assert_eq!(metrics.max_depth, 2);
}

#[test]
fn depth_follows_a_left_deep_spine() {
    // ((Nat × Nat) × Nat) × Nat — the deepest path descends through LEFT
    // children only, pinning the left-child depth bookkeeping.
    let ty = Type::product(
        Type::product(Type::product(Type::Nat, Type::Nat), Type::Nat),
        Type::Nat,
    );
    let metrics = measure_type(&ty);
    assert_eq!(metrics.node_count, 7);
    assert_eq!(metrics.max_depth, 4);
}

#[test]
fn depth_follows_the_deepest_branch() {
    // Nat × (Nat → (Nat + Nat)) — deepest branch has 4 levels.
    let ty = Type::product(
        Type::Nat,
        Type::arrow(Type::Nat, Type::sum(Type::Nat, Type::Nat)),
    );
    let metrics = measure_type(&ty);
    assert_eq!(metrics.node_count, 7);
    assert_eq!(metrics.max_depth, 4);
}

#[test]
fn mu_list_encoding_reports_binder_and_occurrences() {
    // μα_List. Unit + (Nat × α_List) — the classic List<Nat> shape: k = 1.
    let ty = Type::Mu(
        "\u{3b1}_List".to_string(),
        Box::new(Type::sum(
            Type::Unit,
            Type::product(Type::Nat, Type::TyVar("\u{3b1}_List".to_string())),
        )),
    );
    let metrics = measure_type(&ty);
    assert_eq!(metrics.node_count, 6);
    assert_eq!(metrics.max_depth, 4);
    assert_eq!(metrics.mu_binder_chain, vec!["\u{3b1}_List".to_string()]);
    assert_eq!(
        metrics.alpha_occurrences,
        vec![("\u{3b1}_List".to_string(), 1)]
    );
}

#[test]
fn multiple_alpha_occurrences_are_the_k_factor() {
    // μα_Tree. Unit + (α_Tree × α_Tree) — binary tree: k = 2.
    let ty = Type::Mu(
        "\u{3b1}_Tree".to_string(),
        Box::new(Type::sum(
            Type::Unit,
            Type::product(
                Type::TyVar("\u{3b1}_Tree".to_string()),
                Type::TyVar("\u{3b1}_Tree".to_string()),
            ),
        )),
    );
    let metrics = measure_type(&ty);
    assert_eq!(
        metrics.alpha_occurrences,
        vec![("\u{3b1}_Tree".to_string(), 2)]
    );
}

#[test]
fn nested_mu_binders_chain_in_preorder() {
    // μα_Expr. μα_TypeExpr. (α_Expr × α_TypeExpr) — mutual-recursion shape.
    let ty = Type::Mu(
        "\u{3b1}_Expr".to_string(),
        Box::new(Type::Mu(
            "\u{3b1}_TypeExpr".to_string(),
            Box::new(Type::product(
                Type::TyVar("\u{3b1}_Expr".to_string()),
                Type::TyVar("\u{3b1}_TypeExpr".to_string()),
            )),
        )),
    );
    let metrics = measure_type(&ty);
    assert_eq!(
        metrics.mu_binder_chain,
        vec!["\u{3b1}_Expr".to_string(), "\u{3b1}_TypeExpr".to_string()]
    );
    assert_eq!(
        metrics.alpha_occurrences,
        vec![
            ("\u{3b1}_Expr".to_string(), 1),
            ("\u{3b1}_TypeExpr".to_string(), 1)
        ]
    );
}

#[test]
fn unbound_tyvars_are_not_alpha_occurrences() {
    // A free TyVar (no enclosing Mu binder of that name) is not a k factor.
    let ty = Type::Mu(
        "\u{3b1}_List".to_string(),
        Box::new(Type::product(
            Type::TyVar("T".to_string()),
            Type::TyVar("\u{3b1}_List".to_string()),
        )),
    );
    let metrics = measure_type(&ty);
    assert_eq!(
        metrics.alpha_occurrences,
        vec![("\u{3b1}_List".to_string(), 1)]
    );
}

#[test]
fn adt_counts_type_args_and_variant_payloads() {
    // Adt("Option", [Nat], [("None", Unit), ("Some", Nat)])
    let ty = Type::Adt(
        "Option".to_string(),
        vec![Type::Nat],
        vec![
            ("None".to_string(), Type::Unit),
            ("Some".to_string(), Type::Nat),
        ],
    );
    let metrics = measure_type(&ty);
    // Adt node + 1 type arg + 2 payloads.
    assert_eq!(metrics.node_count, 4);
    assert_eq!(metrics.max_depth, 2);
}

#[test]
fn app_counts_argument_subtrees() {
    let ty = Type::App(
        "Forest".to_string(),
        vec![Type::Nat, Type::arrow(Type::Nat, Type::Bool)],
    );
    let metrics = measure_type(&ty);
    // App node + Nat + (Arrow + 2 leaves).
    assert_eq!(metrics.node_count, 5);
    assert_eq!(metrics.max_depth, 3);
}

#[test]
fn eq_walks_type_child_but_not_terms() {
    let ty = Type::Eq(
        Box::new(Type::Nat),
        Box::new(crate::terms::Term::Zero),
        Box::new(crate::terms::Term::Zero),
    );
    let metrics = measure_type(&ty);
    assert_eq!(
        metrics.node_count, 2,
        "Eq node + type child; terms not walked"
    );
    assert_eq!(metrics.max_depth, 2, "the type child sits one level down");
}

#[test]
fn adt_payload_depth_is_tracked_without_type_args() {
    // Depth must flow through variant payloads even when there are no type
    // args to carry it: Adt → Product → Nat is 3 levels.
    let ty = Type::Adt(
        "Pair".to_string(),
        vec![],
        vec![("MkPair".to_string(), Type::product(Type::Nat, Type::Nat))],
    );
    let metrics = measure_type(&ty);
    assert_eq!(metrics.node_count, 4);
    assert_eq!(metrics.max_depth, 3);
}

#[test]
fn adt_type_arg_depth_is_tracked_without_deep_payloads() {
    // Symmetric case: depth must flow through type args when every payload
    // is shallow: Adt → Arrow → Nat is 3 levels.
    let ty = Type::Adt(
        "Box".to_string(),
        vec![Type::arrow(Type::Nat, Type::Nat)],
        vec![("MkBox".to_string(), Type::Unit)],
    );
    let metrics = measure_type(&ty);
    assert_eq!(metrics.node_count, 5);
    assert_eq!(metrics.max_depth, 3);
}

#[test]
fn deep_tree_does_not_overflow_the_stack() {
    // 100k nested Ptr levels — a recursive walker would blow the 2MiB test
    // stack here. Leaked afterwards: the auto-generated recursive Drop would
    // itself overflow on a chain this deep.
    let mut ty = Type::Nat;
    for _ in 0..100_000 {
        ty = Type::Ptr(Box::new(ty));
    }
    let metrics = measure_type(&ty);
    assert_eq!(metrics.node_count, 100_001);
    assert_eq!(metrics.max_depth, 100_001);
    std::mem::forget(ty);
}

#[test]
fn count_nodes_matches_measure() {
    let ty = Type::product(Type::Nat, Type::arrow(Type::Bool, Type::Unit));
    assert_eq!(count_nodes(&ty), measure_type(&ty).node_count);
    assert_eq!(count_nodes(&ty), 5);
}
