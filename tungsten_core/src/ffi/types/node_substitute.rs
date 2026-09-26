//! τ[α := τ'] on the node DAG with structural sharing (ADR 2.7.26a §4).
//!
//! Port of `Type::substitute` to [`TypeNode`] handles. Untouched subtrees
//! return their ORIGINAL handle — only the spine of paths that actually
//! contain `α` allocates new nodes. This is what makes the self-hosted compiler's μ-unfold
//! (`tg_type_substitute`, the hottest type op) retain O(paths-to-α) instead
//! of O(tree) per call. The `substitute_matches_owned_oracle` test pins the
//! port to the owned implementation.

use super::nodes::{materialize_type, TypeNode};
use crate::ffi::{Arena, TypeHandle};
use crate::types::Type;

/// Substitute `var := replacement` in `ty`. Returns the original handle when
/// nothing changed (sharing); `None` on a dangling handle.
pub(crate) fn substitute_type_handle(
    arena: &mut Arena,
    ty: TypeHandle,
    var: &str,
    replacement: TypeHandle,
) -> Option<TypeHandle> {
    arena.get_type_node(replacement)?;
    let mut ctx = SubstCtx {
        var,
        replacement,
        owned_replacement: None,
    };
    ctx.subst(arena, ty)
}

/// One substitution's state: the variable, the replacement handle, and a
/// lazily materialized owned copy of the replacement (needed only when an
/// `Eq` node's terms require `Term::substitute_type`).
struct SubstCtx<'a> {
    var: &'a str,
    replacement: TypeHandle,
    owned_replacement: Option<Type>,
}

impl SubstCtx<'_> {
    /// Flat dispatcher over node shapes; arms delegate to the shape helpers
    /// below, each of which returns the ORIGINAL handle when nothing changed.
    fn subst(&mut self, arena: &mut Arena, ty: TypeHandle) -> Option<TypeHandle> {
        let node = arena.get_type_node(ty)?.clone();
        match node {
            TypeNode::Bool
            | TypeNode::Nat
            | TypeNode::Int
            | TypeNode::Unit
            | TypeNode::Void
            | TypeNode::Prop
            | TypeNode::String
            | TypeNode::Error => Some(ty),
            TypeNode::TyVar(v) => Some(if v == self.var { self.replacement } else { ty }),
            TypeNode::Arrow(a, b) => self.pair(arena, ty, (a, b), TypeNode::Arrow),
            TypeNode::Product(a, b) => self.pair(arena, ty, (a, b), TypeNode::Product),
            TypeNode::Sum(a, b) => self.pair(arena, ty, (a, b), TypeNode::Sum),
            TypeNode::Forall(v, body) => self.binder(arena, ty, (v, body), TypeNode::Forall),
            TypeNode::Mu(v, body) => self.binder(arena, ty, (v, body), TypeNode::Mu),
            TypeNode::Eq(t, lhs, rhs) => self.eq(arena, ty, (t, lhs, rhs)),
            TypeNode::Ptr(inner) => self.unary(arena, ty, inner, TypeNode::Ptr),
            TypeNode::Ref(inner) => self.unary(arena, ty, inner, TypeNode::Ref),
            TypeNode::App(name, args) => self.app(arena, ty, (name, args)),
            TypeNode::Adt(name, type_args, variants) => {
                self.adt(arena, ty, (name, type_args, variants))
            }
        }
    }

    fn pair(
        &mut self,
        arena: &mut Arena,
        ty: TypeHandle,
        (a, b): (TypeHandle, TypeHandle),
        make: fn(TypeHandle, TypeHandle) -> TypeNode,
    ) -> Option<TypeHandle> {
        let (na, nb) = (self.subst(arena, a)?, self.subst(arena, b)?);
        if na == a && nb == b {
            return Some(ty);
        }
        Some(arena.alloc_type_node(make(na, nb)))
    }

    fn unary(
        &mut self,
        arena: &mut Arena,
        ty: TypeHandle,
        inner: TypeHandle,
        make: fn(TypeHandle) -> TypeNode,
    ) -> Option<TypeHandle> {
        let ni = self.subst(arena, inner)?;
        if ni == inner {
            return Some(ty);
        }
        Some(arena.alloc_type_node(make(ni)))
    }

    /// Binder shadowing: no substitution under a binder of the same name.
    fn binder(
        &mut self,
        arena: &mut Arena,
        ty: TypeHandle,
        (v, body): (String, TypeHandle),
        make: fn(String, TypeHandle) -> TypeNode,
    ) -> Option<TypeHandle> {
        if v == self.var {
            return Some(ty);
        }
        let nb = self.subst(arena, body)?;
        if nb == body {
            return Some(ty);
        }
        Some(arena.alloc_type_node(make(v, nb)))
    }

    /// Eq descends into its (owned) terms via `Term::substitute_type`.
    fn eq(
        &mut self,
        arena: &mut Arena,
        ty: TypeHandle,
        (t, lhs, rhs): (TypeHandle, crate::ffi::TermHandle, crate::ffi::TermHandle),
    ) -> Option<TypeHandle> {
        let nt = self.subst(arena, t)?;
        if self.owned_replacement.is_none() {
            self.owned_replacement = Some(materialize_type(arena, self.replacement)?);
        }
        let repl = self.owned_replacement.as_ref().expect("just materialized");
        let old_lhs = crate::ffi::terms::nodes::materialize_term(arena, lhs)?;
        let old_rhs = crate::ffi::terms::nodes::materialize_term(arena, rhs)?;
        let new_lhs = old_lhs.substitute_type(self.var, repl);
        let new_rhs = old_rhs.substitute_type(self.var, repl);
        if nt == t && new_lhs == old_lhs && new_rhs == old_rhs {
            return Some(ty);
        }
        let hl = crate::ffi::terms::nodes::import_term(arena, &new_lhs);
        let hr = crate::ffi::terms::nodes::import_term(arena, &new_rhs);
        Some(arena.alloc_type_node(TypeNode::Eq(nt, hl, hr)))
    }

    fn app(
        &mut self,
        arena: &mut Arena,
        ty: TypeHandle,
        (name, args): (String, Vec<TypeHandle>),
    ) -> Option<TypeHandle> {
        let new_args = args
            .iter()
            .map(|a| self.subst(arena, *a))
            .collect::<Option<Vec<_>>>()?;
        if new_args == args {
            return Some(ty);
        }
        Some(arena.alloc_type_node(TypeNode::App(name, new_args)))
    }

    fn adt(
        &mut self,
        arena: &mut Arena,
        ty: TypeHandle,
        (name, type_args, variants): (String, Vec<TypeHandle>, Vec<(String, TypeHandle)>),
    ) -> Option<TypeHandle> {
        let new_args = type_args
            .iter()
            .map(|a| self.subst(arena, *a))
            .collect::<Option<Vec<_>>>()?;
        let new_variants = variants
            .iter()
            .map(|(ctor, payload)| self.subst(arena, *payload).map(|p| (ctor.clone(), p)))
            .collect::<Option<Vec<_>>>()?;
        if new_args == type_args && new_variants == variants {
            return Some(ty);
        }
        Some(arena.alloc_type_node(TypeNode::Adt(name, new_args, new_variants)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::types::nodes::import_type;
    use crate::terms::Term;

    /// (type, var, replacement) triples covering every arm: leafs, hits,
    /// misses, binder shadowing (Forall/Mu), Eq term descent, App/Adt args.
    fn corpus() -> Vec<(Type, &'static str, Type)> {
        let tv = |s: &str| Type::TyVar(s.into());
        vec![
            (Type::Nat, "a", Type::Bool),
            (tv("a"), "a", Type::Nat),
            (tv("b"), "a", Type::Nat),
            (Type::arrow(tv("a"), tv("b")), "a", Type::Nat),
            (
                Type::product(tv("a"), tv("a")),
                "a",
                Type::arrow(Type::Nat, Type::Bool),
            ),
            (Type::sum(Type::Unit, tv("a")), "a", Type::Void),
            // Shadowed: binder of the same name blocks substitution.
            (Type::Forall("a".into(), Box::new(tv("a"))), "a", Type::Nat),
            (
                Type::Forall("b".into(), Box::new(Type::arrow(tv("a"), tv("b")))),
                "a",
                Type::Nat,
            ),
            (
                Type::Mu("a".into(), Box::new(Type::sum(Type::Unit, tv("a")))),
                "a",
                Type::Nat,
            ),
            // The μ-unfold shape: substitute the binder var in the BODY.
            (
                Type::sum(Type::Unit, Type::product(Type::Nat, tv("α_List"))),
                "α_List",
                Type::Mu(
                    "α_List".into(),
                    Box::new(Type::sum(
                        Type::Unit,
                        Type::product(Type::Nat, tv("α_List")),
                    )),
                ),
            ),
            (
                Type::eq(
                    tv("a"),
                    Term::Zero,
                    Term::Annot(Box::new(Term::Zero), tv("a")),
                ),
                "a",
                Type::Nat,
            ),
            // One-side-changed Eq cases: pin each conjunct of the eq-arm's
            // share detection (type-only, lhs-only, rhs-only changes).
            (Type::eq(tv("a"), Term::Zero, Term::Zero), "a", Type::Nat),
            (
                Type::eq(
                    Type::Nat,
                    Term::Annot(Box::new(Term::Zero), tv("a")),
                    Term::Zero,
                ),
                "a",
                Type::Nat,
            ),
            (
                Type::eq(
                    Type::Nat,
                    Term::Zero,
                    Term::Annot(Box::new(Term::Zero), tv("a")),
                ),
                "a",
                Type::Nat,
            ),
            (Type::ptr(tv("a")), "a", Type::Nat),
            (Type::ref_ty(tv("a")), "a", Type::Nat),
            (
                Type::app("Forest", vec![tv("a"), Type::Nat]),
                "a",
                Type::Bool,
            ),
            (
                Type::adt(
                    "Tree",
                    vec![tv("a")],
                    vec![("Leaf".into(), Type::Unit), ("Node".into(), tv("a"))],
                ),
                "a",
                Type::Nat,
            ),
            // One-side-changed Adt cases: pin each conjunct of the adt-arm's
            // share detection (type-args-only, variants-only changes).
            (
                Type::adt("Tree", vec![tv("a")], vec![("Leaf".into(), Type::Unit)]),
                "a",
                Type::Nat,
            ),
            (
                Type::adt("Tree", vec![Type::Nat], vec![("Node".into(), tv("a"))]),
                "a",
                Type::Nat,
            ),
        ]
    }

    /// The port must produce exactly what the owned `Type::substitute`
    /// produces, for every corpus triple.
    #[test]
    fn substitute_matches_owned_oracle() {
        let mut arena = Arena::new();
        for (ty, var, repl) in corpus() {
            let h_ty = import_type(&mut arena, &ty);
            let h_repl = import_type(&mut arena, &repl);
            let result =
                substitute_type_handle(&mut arena, h_ty, var, h_repl).expect("valid handles");
            let materialized = materialize_type(&arena, result).expect("valid result");
            let oracle = ty.substitute(var, &repl);
            assert_eq!(
                materialized, oracle,
                "substitute diverged from oracle: {ty:?}[{var} := {repl:?}]"
            );
        }
    }

    /// Untouched subtrees must share: substituting a var that does not occur
    /// returns the original handle and allocates nothing.
    #[test]
    fn no_occurrence_shares_the_original_handle() {
        let mut arena = Arena::new();
        let ty = Type::arrow(
            Type::product(Type::Nat, Type::Bool),
            Type::sum(Type::Unit, Type::String),
        );
        let h_ty = import_type(&mut arena, &ty);
        let h_repl = import_type(&mut arena, &Type::Nat);
        let nodes_before = arena.types.len();
        let result = substitute_type_handle(&mut arena, h_ty, "absent", h_repl).unwrap();
        assert_eq!(result, h_ty, "unchanged tree must return the same handle");
        assert_eq!(arena.types.len(), nodes_before, "no new nodes allocated");
    }

    /// A hit deep on one path must rebuild ONLY that spine: siblings share.
    #[test]
    fn partial_hit_rebuilds_only_the_touched_spine() {
        let mut arena = Arena::new();
        let big_untouched = Type::product(
            Type::arrow(Type::Nat, Type::Bool),
            Type::arrow(Type::String, Type::Unit),
        );
        let ty = Type::arrow(big_untouched, Type::TyVar("a".into()));
        let h_ty = import_type(&mut arena, &ty);
        let h_repl = import_type(&mut arena, &Type::Nat);
        let nodes_before = arena.types.len();
        let result = substitute_type_handle(&mut arena, h_ty, "a", h_repl).unwrap();
        // Only the root Arrow is rebuilt (TyVar→replacement shares h_repl,
        // the big left subtree shares wholesale): exactly ONE new node.
        assert_eq!(arena.types.len(), nodes_before + 1);
        let materialized = materialize_type(&arena, result).unwrap();
        assert_eq!(materialized, ty.substitute("a", &Type::Nat));
    }

    /// The eq-arm's share detection must return the ORIGINAL handle when
    /// nothing (type or terms) contains the variable (kills the `==`→`!=`
    /// mutants in its three-way comparison).
    #[test]
    fn absent_var_in_eq_type_shares_the_original_handle() {
        let mut arena = Arena::new();
        let ty = Type::eq(
            Type::arrow(Type::Nat, Type::Bool),
            Term::Zero,
            Term::Annot(Box::new(Term::Zero), Type::Nat),
        );
        let h_ty = import_type(&mut arena, &ty);
        let h_repl = import_type(&mut arena, &Type::Nat);
        let (nodes_before, terms_before) = (arena.types.len(), arena.terms.len());
        let result = substitute_type_handle(&mut arena, h_ty, "absent", h_repl).unwrap();
        assert_eq!(
            result, h_ty,
            "unchanged Eq type must return the same handle"
        );
        assert_eq!(arena.types.len(), nodes_before, "no new type nodes");
        assert_eq!(arena.terms.len(), terms_before, "no new term nodes");
    }

    /// Same for the adt-arm: unchanged type-args AND variants must share
    /// (kills the `==`→`!=` mutants in its two-way comparison).
    #[test]
    fn absent_var_in_adt_type_shares_the_original_handle() {
        let mut arena = Arena::new();
        let ty = Type::adt(
            "Tree",
            vec![Type::Nat],
            vec![("Leaf".into(), Type::Unit), ("Node".into(), Type::Bool)],
        );
        let h_ty = import_type(&mut arena, &ty);
        let h_repl = import_type(&mut arena, &Type::Nat);
        let nodes_before = arena.types.len();
        let result = substitute_type_handle(&mut arena, h_ty, "absent", h_repl).unwrap();
        assert_eq!(
            result, h_ty,
            "unchanged Adt type must return the same handle"
        );
        assert_eq!(arena.types.len(), nodes_before, "no new type nodes");
    }

    #[test]
    fn dangling_handles_return_none() {
        let mut arena = Arena::new();
        let h = import_type(&mut arena, &Type::Nat);
        let invalid = crate::ffi::INVALID_HANDLE;
        assert_eq!(substitute_type_handle(&mut arena, invalid, "a", h), None);
        assert_eq!(substitute_type_handle(&mut arena, h, "a", invalid), None);
    }
}
