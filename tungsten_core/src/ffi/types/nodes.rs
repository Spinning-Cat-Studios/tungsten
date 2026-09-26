//! Handle-children node representation for arena-stored types (ADR 2.7.26a §4).
//!
//! The owned-`Type` arena retained O(N·depth) bytes because every composite
//! FFI constructor deep-cloned both children into its new slot — the arena
//! kept a full copy of every intermediate construction step (measured at
//! ~35% of the 31 GiB self-compiled RSS, §3.4). A [`TypeNode`] stores child *handles*
//! instead, so composing N nodes retains O(N): children are shared, never
//! copied. Owned `Type` trees now exist only transiently, materialized at
//! true consumption boundaries (the bootstrap kernel typechecker, display, debug)
//! and freed by Rust afterwards.
//!
//! Handle semantics are unchanged: opaque, monotonically allocated,
//! phase-local (ADR 10.5.26e). Accessors may now return an existing child
//! handle rather than a fresh copy — handles were never identity-comparable,
//! so aliasing is invisible to the self-hosted compiler.

use crate::types::Type;

use crate::ffi::{Arena, TermHandle, TypeHandle};

/// Arena node mirroring [`Type`], with children as handles.
///
/// `Eq`'s term components stay [`TermHandle`]s into the (still owned) term
/// arena. `App`/`Adt` are unreachable from the self-host constructor surface but
/// are mirrored so kernel-produced types import losslessly.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TypeNode {
    Bool,
    Nat,
    Int,
    Unit,
    Void,
    Prop,
    String,
    Error,
    TyVar(String),
    Arrow(TypeHandle, TypeHandle),
    Product(TypeHandle, TypeHandle),
    Sum(TypeHandle, TypeHandle),
    Forall(String, TypeHandle),
    Mu(String, TypeHandle),
    Eq(TypeHandle, TermHandle, TermHandle),
    Ptr(TypeHandle),
    Ref(TypeHandle),
    App(String, Vec<TypeHandle>),
    Adt(String, Vec<TypeHandle>, Vec<(String, TypeHandle)>),
}

/// Decompose an owned `Type` tree into arena nodes, returning the root
/// handle. Cost: O(tree) nodes retained — used only at import boundaries
/// (kernel typecheck results, `Eq` substitution results).
pub(crate) fn import_type(arena: &mut Arena, ty: &Type) -> TypeHandle {
    let node = match ty {
        Type::Bool => TypeNode::Bool,
        Type::Nat => TypeNode::Nat,
        Type::Int => TypeNode::Int,
        Type::Unit => TypeNode::Unit,
        Type::Void => TypeNode::Void,
        Type::Prop => TypeNode::Prop,
        Type::String => TypeNode::String,
        Type::Error => TypeNode::Error,
        Type::TyVar(v) => TypeNode::TyVar(v.clone()),
        Type::Arrow(a, b) => {
            let (a, b) = (import_type(arena, a), import_type(arena, b));
            TypeNode::Arrow(a, b)
        }
        Type::Product(a, b) => {
            let (a, b) = (import_type(arena, a), import_type(arena, b));
            TypeNode::Product(a, b)
        }
        Type::Sum(a, b) => {
            let (a, b) = (import_type(arena, a), import_type(arena, b));
            TypeNode::Sum(a, b)
        }
        Type::Forall(v, body) => {
            let body = import_type(arena, body);
            TypeNode::Forall(v.clone(), body)
        }
        Type::Mu(v, body) => {
            let body = import_type(arena, body);
            TypeNode::Mu(v.clone(), body)
        }
        Type::Eq(t, lhs, rhs) => {
            let t = import_type(arena, t);
            let lhs = crate::ffi::terms::nodes::import_term(arena, lhs);
            let rhs = crate::ffi::terms::nodes::import_term(arena, rhs);
            TypeNode::Eq(t, lhs, rhs)
        }
        Type::Ptr(inner) => {
            let inner = import_type(arena, inner);
            TypeNode::Ptr(inner)
        }
        Type::Ref(inner) => {
            let inner = import_type(arena, inner);
            TypeNode::Ref(inner)
        }
        Type::App(name, args) => {
            let args = args.iter().map(|a| import_type(arena, a)).collect();
            TypeNode::App(name.clone(), args)
        }
        Type::Adt(name, type_args, variants) => {
            let type_args = type_args.iter().map(|a| import_type(arena, a)).collect();
            let variants = variants
                .iter()
                .map(|(ctor, payload)| (ctor.clone(), import_type(arena, payload)))
                .collect();
            TypeNode::Adt(name.clone(), type_args, variants)
        }
    };
    arena.alloc_type_node(node)
}

/// Rebuild an owned `Type` tree from arena nodes. Cost: O(tree) transient —
/// the result is freed by the caller (kernel typecheck input, display,
/// debug). `None` on an invalid/dangling handle.
pub(crate) fn materialize_type(arena: &Arena, handle: TypeHandle) -> Option<Type> {
    let ty = match arena.get_type_node(handle)? {
        TypeNode::Bool => Type::Bool,
        TypeNode::Nat => Type::Nat,
        TypeNode::Int => Type::Int,
        TypeNode::Unit => Type::Unit,
        TypeNode::Void => Type::Void,
        TypeNode::Prop => Type::Prop,
        TypeNode::String => Type::String,
        TypeNode::Error => Type::Error,
        TypeNode::TyVar(v) => Type::TyVar(v.clone()),
        TypeNode::Arrow(a, b) => {
            Type::arrow(materialize_type(arena, *a)?, materialize_type(arena, *b)?)
        }
        TypeNode::Product(a, b) => {
            Type::product(materialize_type(arena, *a)?, materialize_type(arena, *b)?)
        }
        TypeNode::Sum(a, b) => {
            Type::sum(materialize_type(arena, *a)?, materialize_type(arena, *b)?)
        }
        TypeNode::Forall(v, body) => {
            Type::Forall(v.clone(), Box::new(materialize_type(arena, *body)?))
        }
        TypeNode::Mu(v, body) => Type::Mu(v.clone(), Box::new(materialize_type(arena, *body)?)),
        TypeNode::Eq(t, lhs, rhs) => Type::eq(
            materialize_type(arena, *t)?,
            crate::ffi::terms::nodes::materialize_term(arena, *lhs)?,
            crate::ffi::terms::nodes::materialize_term(arena, *rhs)?,
        ),
        TypeNode::Ptr(inner) => Type::ptr(materialize_type(arena, *inner)?),
        TypeNode::Ref(inner) => Type::ref_ty(materialize_type(arena, *inner)?),
        TypeNode::App(name, args) => Type::app(
            name.clone(),
            args.iter()
                .map(|a| materialize_type(arena, *a))
                .collect::<Option<Vec<_>>>()?,
        ),
        TypeNode::Adt(name, type_args, variants) => Type::adt(
            name.clone(),
            type_args
                .iter()
                .map(|a| materialize_type(arena, *a))
                .collect::<Option<Vec<_>>>()?,
            variants
                .iter()
                .map(|(ctor, payload)| materialize_type(arena, *payload).map(|p| (ctor.clone(), p)))
                .collect::<Option<Vec<_>>>()?,
        ),
    };
    Some(ty)
}

/// Heap bytes owned by ONE node (strings + handle-vec slabs) — the node
/// arena's per-allocation retention contribution (ADR 2.7.26a §3.4
/// accounting, node semantics: no child recursion, children are shared).
pub(crate) fn node_heap_bytes(node: &TypeNode) -> u64 {
    let handle_size = size_of::<TypeHandle>() as u64;
    match node {
        TypeNode::Bool
        | TypeNode::Nat
        | TypeNode::Int
        | TypeNode::Unit
        | TypeNode::Void
        | TypeNode::Prop
        | TypeNode::String
        | TypeNode::Error
        | TypeNode::Arrow(..)
        | TypeNode::Product(..)
        | TypeNode::Sum(..)
        | TypeNode::Eq(..)
        | TypeNode::Ptr(..)
        | TypeNode::Ref(..) => 0,
        TypeNode::TyVar(v) | TypeNode::Forall(v, _) | TypeNode::Mu(v, _) => v.capacity() as u64,
        TypeNode::App(name, args) => {
            name.capacity() as u64 + (args.capacity() as u64) * handle_size
        }
        TypeNode::Adt(name, type_args, variants) => {
            name.capacity() as u64
                + (type_args.capacity() as u64) * handle_size
                + (variants.capacity() as u64) * size_of::<(String, TypeHandle)>() as u64
                + variants
                    .iter()
                    .map(|(n, _)| n.capacity() as u64)
                    .sum::<u64>()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terms::Term;

    fn arena() -> Arena {
        Arena::new()
    }

    fn sample_types() -> Vec<Type> {
        vec![
            Type::Nat,
            Type::Int,
            Type::Error,
            Type::TyVar("@Cursor".into()),
            Type::arrow(Type::Nat, Type::Bool),
            Type::product(Type::TyVar("a".into()), Type::String),
            Type::sum(Type::Unit, Type::Void),
            Type::Forall(
                "t".into(),
                Box::new(Type::arrow(
                    Type::TyVar("t".into()),
                    Type::TyVar("t".into()),
                )),
            ),
            Type::Mu(
                "α_List".into(),
                Box::new(Type::sum(
                    Type::Unit,
                    Type::product(Type::Nat, Type::TyVar("α_List".into())),
                )),
            ),
            Type::eq(Type::Nat, Term::Zero, Term::Succ(Box::new(Term::Zero))),
            Type::ptr(Type::Nat),
            Type::ref_ty(Type::Bool),
            Type::app("Forest", vec![Type::TyVar("t".into())]),
            Type::adt(
                "Tree",
                vec![Type::Nat],
                vec![("Leaf".into(), Type::Unit), ("Node".into(), Type::Nat)],
            ),
        ]
    }

    #[test]
    fn import_materialize_round_trips_every_variant() {
        let mut arena = arena();
        for ty in sample_types() {
            let handle = import_type(&mut arena, &ty);
            let back = materialize_type(&arena, handle).expect("valid handle");
            assert_eq!(back, ty, "round-trip mismatch for {ty:?}");
        }
    }

    #[test]
    fn materialize_rejects_dangling_handles() {
        let arena = arena();
        assert_eq!(materialize_type(&arena, 0), None);
        assert_eq!(materialize_type(&arena, crate::ffi::INVALID_HANDLE), None);
    }

    #[test]
    fn composing_shares_children_instead_of_copying() {
        let mut arena = arena();
        let leaf = import_type(&mut arena, &Type::Nat);
        let nodes_after_leaf = arena.types.len();
        // Build a 64-deep chain of arrows over the same leaf. Owned-tree
        // storage retained O(2^depth) bytes for this shape; nodes must add
        // exactly ONE node per compose step.
        let mut current = leaf;
        for _ in 0..64 {
            let node = TypeNode::Arrow(current, current);
            current = arena.alloc_type_node(node);
        }
        assert_eq!(arena.types.len(), nodes_after_leaf + 64);
    }

    #[test]
    fn node_heap_bytes_counts_strings_and_slabs() {
        let mut name = String::from("Forest");
        name.shrink_to_fit();
        let name_cap = name.capacity() as u64;
        let args = vec![0 as TypeHandle, 1 as TypeHandle];
        let slab = (args.capacity() as u64) * size_of::<TypeHandle>() as u64;
        assert_eq!(node_heap_bytes(&TypeNode::App(name, args)), name_cap + slab);
        assert_eq!(node_heap_bytes(&TypeNode::Arrow(0, 1)), 0);
    }
}
