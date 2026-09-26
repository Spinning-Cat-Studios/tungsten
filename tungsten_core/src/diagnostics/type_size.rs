//! Stored-`Type`-tree size metrics (ADR 8.7.26a §2.3).
//!
//! `tungsten info type encoding` prints the per-constructor *display* form,
//! which under-represents the real stored [`Type`] value — an `Adt` embeds
//! full variant lists in every reference, and that tree is what drives the
//! ADR 7.7.26k unfold blowup (∏ kᵢ). This walker reports the raw tree:
//! total node count, depth, μ-binder nesting chain, and the α-occurrence
//! count per binder (the kᵢ factors).
//!
//! The walk is iterative (explicit stack) — stored trees can be deep enough
//! that recursion would risk the same stack pressure the metrics diagnose.

use std::collections::HashMap;

use crate::types::Type;

/// Size metrics for one stored `Type` tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSizeMetrics {
    /// Total `Type` nodes in the tree. `Eq`'s embedded *terms* are not
    /// walked — an `Eq` node counts as one node plus its type child.
    pub node_count: usize,
    /// Maximum nesting depth (a lone leaf is depth 1).
    pub max_depth: usize,
    /// μ-binder variables in first-encounter (pre-order) order, e.g.
    /// `["α_Expr", "α_TypeExpr"]` for nested `Mu` binders.
    pub mu_binder_chain: Vec<String>,
    /// Per-binder occurrence count of the bound `TyVar` — the kᵢ factors of
    /// the ADR 7.7.26k unfold estimate ∏ kᵢ. Same order as
    /// [`Self::mu_binder_chain`].
    pub alpha_occurrences: Vec<(String, usize)>,
}

/// Walk `ty` and collect its size metrics.
#[must_use]
pub fn measure_type(ty: &Type) -> TypeSizeMetrics {
    let mut node_count = 0usize;
    let mut max_depth = 0usize;
    let mut mu_binder_chain: Vec<String> = Vec::new();
    let mut occurrence_counts: HashMap<String, usize> = HashMap::new();

    let mut pending: Vec<(&Type, usize)> = vec![(ty, 1)];
    while let Some((node, depth)) = pending.pop() {
        node_count += 1;
        max_depth = max_depth.max(depth);
        match node {
            Type::Bool
            | Type::Nat
            | Type::Int
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::String
            | Type::Error => {}
            Type::TyVar(var) => {
                if let Some(count) = occurrence_counts.get_mut(var) {
                    *count += 1;
                }
            }
            Type::Arrow(left, right) | Type::Product(left, right) | Type::Sum(left, right) => {
                pending.push((right, depth + 1));
                pending.push((left, depth + 1));
            }
            Type::Forall(_, body) | Type::Ptr(body) | Type::Ref(body) => {
                pending.push((body, depth + 1));
            }
            Type::Mu(var, body) => {
                // First encounter of a binder starts its occurrence count;
                // α_-prefixed vars are unique per type name, so a repeated
                // binder (re-encoded subtree) keeps accumulating into the
                // same counter rather than resetting it.
                if !occurrence_counts.contains_key(var) {
                    mu_binder_chain.push(var.clone());
                    occurrence_counts.insert(var.clone(), 0);
                }
                pending.push((body, depth + 1));
            }
            // Eq embeds terms; only the type child is walked (see struct docs).
            Type::Eq(inner, _, _) => pending.push((inner, depth + 1)),
            Type::App(_, args) => {
                for arg in args.iter().rev() {
                    pending.push((arg, depth + 1));
                }
            }
            Type::Adt(_, type_args, variants) => {
                for (_, payload) in variants.iter().rev() {
                    pending.push((payload, depth + 1));
                }
                for arg in type_args.iter().rev() {
                    pending.push((arg, depth + 1));
                }
            }
        }
    }

    let alpha_occurrences = mu_binder_chain
        .iter()
        .map(|binder| (binder.clone(), occurrence_counts[binder]))
        .collect();
    TypeSizeMetrics {
        node_count,
        max_depth,
        mu_binder_chain,
        alpha_occurrences,
    }
}

/// Node count only — for per-variant / per-field breakdowns.
#[must_use]
pub fn count_nodes(ty: &Type) -> usize {
    measure_type(ty).node_count
}

// Tests: type_size_tests.rs
#[cfg(test)]
#[path = "type_size_tests.rs"]
mod type_size_tests;
