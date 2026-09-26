//! Type introspection predicates for FFI.
//!
//! Provides C-compatible type predicate functions (`tg_type_is_*`).
//! Type accessors, substitution, and debug utilities are in `types_accessors.rs`.
//!
//! `predicates.rs` became `predicates/` when ADR 13.8.26a's tag-table
//! reconciliation took it past the file-size cap. The seam is production FFI
//! here, the doc-comment↔arms reconciliation in [`tag_table_tests`] — which has
//! to live in this directory, since it `include_str!`s this file.

use super::nodes::TypeNode;
use crate::ffi::{with_arena_ref, TypeHandle};

// ============================================================================
// Type Tag (discriminant)
// ============================================================================

/// Return a numeric tag identifying the top-level variant of a type.
///
/// **This is the authority for the tag table** (ADR 13.8.26a). Every other
/// mention of these numbers in the repo points here rather than re-listing
/// them; the one surviving transcription — `docs/repo-memory/codegen-pipeline.md`
/// § TypeHandle → CodegenType Bridge — is reconciled against these arms by
/// `code-health`'s `tag-table-docs`. The list below is reconciled **twice** — by
/// `tag_table_tests::the_documented_tag_table_matches_the_match_arms` under
/// `cargo test`, and by that same `tag-table-docs` check under
/// `make check-health` — so it is a refutable claim rather than a comment, and
/// deleting either guard does not quietly return it to being unguarded.
///
/// Tags:
///   0 = Nat, 1 = Bool, 2 = String, 3 = Unit, 4 = Void, 5 = Prop,
///   6 = Arrow, 7 = Product, 8 = Sum, 9 = TyVar, 10 = Forall,
///   11 = Mu, 12 = Eq, 13 = Ref, 14 = Ptr, 15 = App, 16 = Adt, 17 = Int,
///   99 = unknown / invalid handle
#[no_mangle]
pub extern "C" fn tg_type_tag(ty: TypeHandle) -> u64 {
    with_arena_ref!(|arena| {
        match arena.get_type_node(ty) {
            Some(TypeNode::Nat) => 0,
            Some(TypeNode::Bool) => 1,
            Some(TypeNode::String) => 2,
            Some(TypeNode::Unit) => 3,
            Some(TypeNode::Void) => 4,
            Some(TypeNode::Prop) => 5,
            Some(TypeNode::Arrow(_, _)) => 6,
            Some(TypeNode::Product(_, _)) => 7,
            Some(TypeNode::Sum(_, _)) => 8,
            Some(TypeNode::TyVar(_)) => 9,
            Some(TypeNode::Forall(_, _)) => 10,
            Some(TypeNode::Mu(_, _)) => 11,
            Some(TypeNode::Eq(_, _, _)) => 12,
            Some(TypeNode::Ref(_)) => 13,
            Some(TypeNode::Ptr(_)) => 14,
            Some(TypeNode::App(_, _)) => 15,
            Some(TypeNode::Adt(_, _, _)) => 16,
            // Appended, not slotted beside `Nat`: the table is positional for
            // every `.tg` reader (ADR 14.9.26c §2.1).
            Some(TypeNode::Int) => 17,
            Some(TypeNode::Error) => 99,
            None => 99,
        }
    })
}

// ============================================================================
// Type Predicates (Phase 3C-5)
// ============================================================================

/// Check if a type is a μ-type (recursive type)
#[no_mangle]
pub extern "C" fn tg_type_is_mu(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::Mu(_, _))) })
}

/// Check if a type is a sum type
#[no_mangle]
pub extern "C" fn tg_type_is_sum(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::Sum(_, _))) })
}

/// Check if a type is a product type
#[no_mangle]
pub extern "C" fn tg_type_is_product(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::Product(_, _))) })
}

/// Check if a type is an arrow (function) type
#[no_mangle]
pub extern "C" fn tg_type_is_arrow(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::Arrow(_, _))) })
}

/// Check if a type is an equality type (Eq τ t₁ t₂)
#[no_mangle]
pub extern "C" fn tg_type_is_eq(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::Eq(_, _, _))) })
}

/// Check if a type is a forall type (∀α. τ)
#[no_mangle]
pub extern "C" fn tg_type_is_forall(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::Forall(_, _))) })
}

/// Check if a type is a type variable (named type like record names).
#[no_mangle]
pub extern "C" fn tg_type_is_tyvar(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::TyVar(_))) })
}

/// Check if a type is a type application (parametric type like List<T>).
#[no_mangle]
pub extern "C" fn tg_type_is_app(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::App(_, _))) })
}

// Tests: tag_table_tests.rs
#[cfg(test)]
#[path = "tag_table_tests.rs"]
mod tag_table_tests;
