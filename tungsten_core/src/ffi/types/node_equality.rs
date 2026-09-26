//! α-equivalent type equality directly on the node DAG (ADR 2.7.26a §4).
//!
//! Line-by-line port of `types::equality::types_equal_with_env` to
//! [`TypeNode`] handles — `tg_types_equal` is called from ~90 self-host elaboration
//! sites, so materializing both sides per call would replace the removed
//! retention cost with a transient-allocation storm. The
//! `handle_equality_matches_owned_oracle` test keeps this port pinned to the
//! owned implementation.

use std::collections::HashMap;

use super::nodes::TypeNode;
use crate::ffi::{Arena, TypeHandle};

/// α-equivalence on node handles; mirrors `types_equal_alpha`.
/// Invalid handles are unequal to everything (the FFI boundary's contract).
pub(crate) fn type_handles_equal(arena: &Arena, a: TypeHandle, b: TypeHandle) -> bool {
    // Dangling handles fall out of the walk's node lookups (never equal).
    equal_with_env(arena, a, b, &mut HashMap::new())
}

/// Strip the `@` prefix from named-type TyVars (ADR 13.4.26c §2).
fn strip_named(name: &str) -> &str {
    name.strip_prefix('@').unwrap_or(name)
}

/// TyVar-name vs μ-binder match, honouring the `α_` convention and env.
fn tyvar_matches_mu(v: &str, mu_var: &str, env: &HashMap<String, String>) -> bool {
    let v = strip_named(v);
    let target = mu_var.strip_prefix("α_").unwrap_or(mu_var);
    env.get(v)
        .map_or(v == target, |mapped| strip_named(mapped) == target)
}

fn equal_with_env(
    arena: &Arena,
    a: TypeHandle,
    b: TypeHandle,
    env: &mut HashMap<String, String>,
) -> bool {
    // Same handle ⇒ same node ⇒ equal under any env-consistent reading is
    // NOT generally true (env maps a-side names to b-side names), so no
    // handle fast path — semantics first (mirrors the owned walk exactly).
    let (Some(na), Some(nb)) = (arena.get_type_node(a), arena.get_type_node(b)) else {
        return false;
    };
    match (na, nb) {
        (TypeNode::Bool, TypeNode::Bool)
        | (TypeNode::Nat, TypeNode::Nat)
        | (TypeNode::Int, TypeNode::Int)
        | (TypeNode::Unit, TypeNode::Unit)
        | (TypeNode::Void, TypeNode::Void)
        | (TypeNode::Prop, TypeNode::Prop)
        | (TypeNode::String, TypeNode::String) => true,

        (TypeNode::TyVar(v1), TypeNode::TyVar(v2)) => tyvars_equal(v1, v2, env),

        // 0-arity App is equivalent to TyVar
        (TypeNode::TyVar(v), TypeNode::App(name, args))
        | (TypeNode::App(name, args), TypeNode::TyVar(v)) => tyvar_app_equal(v, name, args, env),

        // TyVar vs Mu — normalization depth asymmetry
        (TypeNode::TyVar(v), TypeNode::Mu(mu_var, _))
        | (TypeNode::Mu(mu_var, _), TypeNode::TyVar(v)) => tyvar_matches_mu(v, mu_var, env),

        (TypeNode::Arrow(a1, a2), TypeNode::Arrow(b1, b2))
        | (TypeNode::Product(a1, a2), TypeNode::Product(b1, b2))
        | (TypeNode::Sum(a1, a2), TypeNode::Sum(b1, b2)) => {
            let (a1, a2, b1, b2) = (*a1, *a2, *b1, *b2);
            equal_with_env(arena, a1, b1, env) && equal_with_env(arena, a2, b2, env)
        }

        (TypeNode::Mu(v1, body1), TypeNode::Mu(v2, body2))
        | (TypeNode::Forall(v1, body1), TypeNode::Forall(v2, body2)) => {
            let (v1, v2, body1, body2) = (v1.clone(), v2.clone(), *body1, *body2);
            with_binding(env, &v1, &v2, |env| {
                equal_with_env(arena, body1, body2, env)
            })
        }

        // Eq types: same underlying type + structurally equal (owned) terms.
        (TypeNode::Eq(ty1, l1, r1), TypeNode::Eq(ty2, l2, r2)) => {
            let (ty1, ty2, l1, r1, l2, r2) = (*ty1, *ty2, *l1, *r1, *l2, *r2);
            equal_with_env(arena, ty1, ty2, env)
                && terms_materialize_equal(arena, l1, l2)
                && terms_materialize_equal(arena, r1, r2)
        }

        (TypeNode::Ptr(i1), TypeNode::Ptr(i2)) | (TypeNode::Ref(i1), TypeNode::Ref(i2)) => {
            let (i1, i2) = (*i1, *i2);
            equal_with_env(arena, i1, i2, env)
        }

        (TypeNode::App(name1, args1), TypeNode::App(name2, args2)) => {
            name1 == name2 && all_equal(arena, &args1.clone(), &args2.clone(), env)
        }

        (TypeNode::Adt(name1, args1, vars1), TypeNode::Adt(name2, args2, vars2)) => {
            if name1 != name2 || args1.len() != args2.len() || vars1.len() != vars2.len() {
                return false;
            }
            let (args1, args2) = (args1.clone(), args2.clone());
            let ctors_match = vars1
                .iter()
                .zip(vars2.iter())
                .all(|((c1, _), (c2, _))| c1 == c2);
            let payloads: Vec<(TypeHandle, TypeHandle)> = vars1
                .iter()
                .zip(vars2.iter())
                .map(|((_, p1), (_, p2))| (*p1, *p2))
                .collect();
            ctors_match
                && all_equal(arena, &args1, &args2, env)
                && payloads
                    .iter()
                    .all(|(p1, p2)| equal_with_env(arena, *p1, *p2, env))
        }

        // TypeError (poison) unifies with any type
        (TypeNode::Error, _) | (_, TypeNode::Error) => true,

        _ => false,
    }
}

/// Structural (non-alpha) term comparison via transient materialization —
/// Eq-type term components are a cold path (proof code only).
fn terms_materialize_equal(
    arena: &Arena,
    a: crate::ffi::TermHandle,
    b: crate::ffi::TermHandle,
) -> bool {
    use crate::ffi::terms::nodes::materialize_term;
    match (materialize_term(arena, a), materialize_term(arena, b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn tyvars_equal(v1: &str, v2: &str, env: &HashMap<String, String>) -> bool {
    let v1 = strip_named(v1);
    let v2 = strip_named(v2);
    env.get(v1)
        .map_or(v1 == v2, |mapped| strip_named(mapped) == v2)
}

fn tyvar_app_equal(
    v: &str,
    name: &str,
    args: &[TypeHandle],
    env: &HashMap<String, String>,
) -> bool {
    if !args.is_empty() {
        return false;
    }
    let v = strip_named(v);
    env.get(v)
        .map_or(v == name, |mapped| strip_named(mapped) == name)
}

fn with_binding<R>(
    env: &mut HashMap<String, String>,
    v1: &str,
    v2: &str,
    f: impl FnOnce(&mut HashMap<String, String>) -> R,
) -> R {
    let old = env.insert(v1.to_owned(), v2.to_owned());
    let result = f(env);
    match old {
        Some(prev) => {
            env.insert(v1.to_owned(), prev);
        }
        None => {
            env.remove(v1);
        }
    }
    result
}

fn all_equal(
    arena: &Arena,
    a: &[TypeHandle],
    b: &[TypeHandle],
    env: &mut HashMap<String, String>,
) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| equal_with_env(arena, *x, *y, env))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::types::nodes::import_type;
    use crate::terms::Term;
    use crate::types::{types_equal_alpha, Type};

    /// A corpus spanning every equality rule: base, TyVar/@-prefix, TyVar↔App,
    /// TyVar↔Mu (α_ convention), binders (α-renaming both ways), Eq terms,
    /// wrappers, App/Adt, poison, and definite inequalities.
    fn corpus() -> Vec<(Type, Type)> {
        let mut pairs = corpus_base_rules();
        pairs.extend(corpus_guards_and_binders());
        pairs
    }

    /// Base types, TyVar conventions, binary/wrapper shapes, Eq terms.
    fn corpus_base_rules() -> Vec<(Type, Type)> {
        let list_mu = |var: &str| {
            Type::Mu(
                var.into(),
                Box::new(Type::sum(
                    Type::Unit,
                    Type::product(Type::Nat, Type::TyVar(var.into())),
                )),
            )
        };
        vec![
            (Type::Nat, Type::Nat),
            (Type::Nat, Type::Bool),
            (Type::TyVar("@Cursor".into()), Type::TyVar("Cursor".into())),
            (Type::TyVar("T".into()), Type::app("T", vec![])),
            (Type::TyVar("T".into()), Type::app("T", vec![Type::Nat])),
            (Type::TyVar("List".into()), list_mu("α_List")),
            (Type::TyVar("Wrong".into()), list_mu("α_List")),
            (list_mu("α_List"), list_mu("β")),
            (
                Type::Forall("a".into(), Box::new(Type::TyVar("a".into()))),
                Type::Forall("b".into(), Box::new(Type::TyVar("b".into()))),
            ),
            (
                Type::Forall("a".into(), Box::new(Type::TyVar("a".into()))),
                Type::Forall("b".into(), Box::new(Type::TyVar("c".into()))),
            ),
            (
                Type::arrow(Type::Nat, Type::Bool),
                Type::arrow(Type::Nat, Type::Bool),
            ),
            (
                Type::arrow(Type::Nat, Type::Bool),
                Type::arrow(Type::Bool, Type::Nat),
            ),
            (
                Type::eq(Type::Nat, Term::Zero, Term::Zero),
                Type::eq(Type::Nat, Term::Zero, Term::Zero),
            ),
            (
                Type::eq(Type::Nat, Term::Zero, Term::Zero),
                Type::eq(Type::Nat, Term::Zero, Term::Succ(Box::new(Term::Zero))),
            ),
            // Type-part differs, terms match: pins the Eq arm's first conjunct.
            (
                Type::eq(Type::Nat, Term::Zero, Term::Zero),
                Type::eq(Type::Bool, Term::Zero, Term::Zero),
            ),
            (Type::ptr(Type::Nat), Type::ptr(Type::Nat)),
            (Type::ptr(Type::Nat), Type::ref_ty(Type::Nat)),
            (
                Type::app("Forest", vec![Type::Nat]),
                Type::app("Forest", vec![Type::Nat]),
            ),
            (
                Type::app("Forest", vec![Type::Nat]),
                Type::app("Forest", vec![Type::Bool]),
            ),
            (
                Type::adt("Tree", vec![Type::Nat], vec![("Leaf".into(), Type::Unit)]),
                Type::adt("Tree", vec![Type::Nat], vec![("Leaf".into(), Type::Unit)]),
            ),
            (
                Type::adt("Tree", vec![], vec![("Leaf".into(), Type::Unit)]),
                Type::adt("Tree", vec![], vec![("Node".into(), Type::Unit)]),
            ),
            (Type::Error, Type::arrow(Type::Nat, Type::Bool)),
            (
                Type::product(Type::String, Type::Unit),
                Type::product(Type::String, Type::Void),
            ),
        ]
    }

    /// Guard clauses (App/Adt), one-side-differs conjuncts, env-mapped
    /// binder equivalences.
    fn corpus_guards_and_binders() -> Vec<(Type, Type)> {
        let list_mu = |var: &str| {
            Type::Mu(
                var.into(),
                Box::new(Type::sum(
                    Type::Unit,
                    Type::product(Type::Nat, Type::TyVar(var.into())),
                )),
            )
        };
        vec![
            // One-side-differs pairs: pin each conjunct of the binary arms.
            (
                Type::arrow(Type::Nat, Type::Bool),
                Type::arrow(Type::Nat, Type::Nat),
            ),
            (
                Type::arrow(Type::Bool, Type::Bool),
                Type::arrow(Type::Nat, Type::Bool),
            ),
            // App arg-count vs name mismatches (pin each guard clause).
            (
                Type::app("Forest", vec![Type::Nat]),
                Type::app("Forest", vec![Type::Nat, Type::Bool]),
            ),
            (
                Type::app("Forest", vec![Type::Nat]),
                Type::app("Grove", vec![Type::Nat]),
            ),
            // Adt guard clauses: name / type-arg count / variant count.
            (
                Type::adt("Tree", vec![], vec![("Leaf".into(), Type::Unit)]),
                Type::adt("Bush", vec![], vec![("Leaf".into(), Type::Unit)]),
            ),
            (
                Type::adt("Tree", vec![Type::Nat], vec![("Leaf".into(), Type::Unit)]),
                Type::adt("Tree", vec![], vec![("Leaf".into(), Type::Unit)]),
            ),
            (
                Type::adt("Tree", vec![], vec![("Leaf".into(), Type::Unit)]),
                Type::adt(
                    "Tree",
                    vec![],
                    vec![("Leaf".into(), Type::Unit), ("Node".into(), Type::Nat)],
                ),
            ),
            // Env-mapped TyVar↔Mu and TyVar↔App under enclosing binders.
            (
                Type::Forall("a".into(), Box::new(Type::TyVar("a".into()))),
                Type::Forall("X".into(), Box::new(list_mu("α_X"))),
            ),
            (
                Type::Forall("a".into(), Box::new(Type::TyVar("a".into()))),
                Type::Forall("N".into(), Box::new(Type::app("N", vec![]))),
            ),
            (
                Type::Forall("a".into(), Box::new(Type::TyVar("a".into()))),
                Type::Forall("N".into(), Box::new(Type::app("M", vec![]))),
            ),
        ]
    }

    /// The port must agree with the owned oracle on every corpus pair,
    /// in both argument orders.
    #[test]
    fn handle_equality_matches_owned_oracle() {
        let mut arena = Arena::new();
        for (a, b) in corpus() {
            let ha = import_type(&mut arena, &a);
            let hb = import_type(&mut arena, &b);
            let oracle = types_equal_alpha(&a, &b);
            assert_eq!(
                type_handles_equal(&arena, ha, hb),
                oracle,
                "handle equality diverged from oracle for {a:?} vs {b:?}"
            );
            assert_eq!(
                type_handles_equal(&arena, hb, ha),
                types_equal_alpha(&b, &a),
                "handle equality diverged from oracle (reversed) for {b:?} vs {a:?}"
            );
        }
    }

    #[test]
    fn invalid_handles_are_never_equal() {
        let mut arena = Arena::new();
        let nat = import_type(&mut arena, &Type::Nat);
        let invalid = crate::ffi::INVALID_HANDLE;
        assert!(!type_handles_equal(&arena, nat, invalid));
        assert!(!type_handles_equal(&arena, invalid, nat));
        assert!(!type_handles_equal(&arena, invalid, invalid));
    }

    /// Sharing must not confuse the env logic: the same shared child handle
    /// compared under different binder mappings.
    #[test]
    fn shared_children_compare_correctly_under_binders() {
        let mut arena = Arena::new();
        let a = Type::Forall("a".into(), Box::new(Type::TyVar("a".into())));
        let b = Type::Forall("b".into(), Box::new(Type::TyVar("b".into())));
        let ha = import_type(&mut arena, &a);
        let hb = import_type(&mut arena, &b);
        assert!(type_handles_equal(&arena, ha, hb));
        // Same handle on both sides is equal (identity ⇒ α-equal).
        assert!(type_handles_equal(&arena, ha, ha));
    }
}
