//! Alpha-equivalent type equality.
//!
//! Checks whether two types are equal up to renaming of bound variables.
//! Handles μ-types, ∀-types, TyVar/App normalization, and TyVar/Mu equivalence.

use super::Type;
use std::collections::HashMap;

/// Check if two types are α-equivalent (equal up to renaming of bound variables).
///
/// This handles:
/// - μα.F ≡ μβ.F[β/α] (μ-types with different bound variable names)
/// - ∀α.τ ≡ ∀β.τ[β/α] (forall types with different bound variable names)
#[must_use]
pub fn types_equal_alpha(a: &Type, b: &Type) -> bool {
    types_equal_with_env(a, b, &mut HashMap::new())
}

/// Strip the `@` prefix from named type TyVars (ADR 13.4.26c §2).
/// Named types use `@` prefix to distinguish from genuine type variables,
/// but comparison should treat `@X` and `X` as the same named type.
fn strip_named(name: &str) -> &str {
    name.strip_prefix('@').unwrap_or(name)
}

/// Check whether a TyVar name matches a Mu recursion variable.
///
/// Handles both the α_ naming convention (`α_X` → `X`) and the self-hosted
/// compiler convention where the Mu variable name matches directly.
fn tyvar_matches_mu(v: &str, mu_var: &str, env: &HashMap<String, String>) -> bool {
    let v = strip_named(v);
    let target = mu_var.strip_prefix("α_").unwrap_or(mu_var);
    env.get(v)
        .map_or(v == target, |mapped| strip_named(mapped) == target)
}

/// Check type equality with an environment mapping bound variables.
///
/// The `env` maps bound variables in `a` to their corresponding bound variables in `b`.
/// Only populated when descending into Mu or Forall binders—free variables
/// are compared by name equality, not tracked here.
pub(super) fn types_equal_with_env(a: &Type, b: &Type, env: &mut HashMap<String, String>) -> bool {
    match (a, b) {
        // Base types
        (Type::Bool, Type::Bool)
        | (Type::Nat, Type::Nat)
        | (Type::Unit, Type::Unit)
        | (Type::Void, Type::Void)
        | (Type::Prop, Type::Prop)
        | (Type::String, Type::String) => true,

        // Type variables
        (Type::TyVar(v1), Type::TyVar(v2)) => tyvars_equal(v1, v2, env),

        // 0-arity App is equivalent to TyVar
        (Type::TyVar(v), Type::App(name, args)) | (Type::App(name, args), Type::TyVar(v)) => {
            tyvar_app_equal(v, name, args, env)
        }

        // TyVar vs Mu — handles normalization depth asymmetry
        (Type::TyVar(v), Type::Mu(mu_var, _)) | (Type::Mu(mu_var, _), Type::TyVar(v)) => {
            tyvar_matches_mu(v, mu_var, env)
        }

        // Binary recursive type constructors
        (Type::Arrow(a1, a2), Type::Arrow(b1, b2))
        | (Type::Product(a1, a2), Type::Product(b1, b2))
        | (Type::Sum(a1, a2), Type::Sum(b1, b2)) => {
            types_equal_with_env(a1, b1, env) && types_equal_with_env(a2, b2, env)
        }

        // Binding forms (μ / ∀): bind variables and compare bodies
        (Type::Mu(v1, body1), Type::Mu(v2, body2))
        | (Type::Forall(v1, body1), Type::Forall(v2, body2)) => {
            with_binding(env, v1, v2, |env| types_equal_with_env(body1, body2, env))
        }

        // Equality types (quasi-dependent)
        (Type::Eq(ty1, t1a, t1b), Type::Eq(ty2, t2a, t2b)) => {
            eq_types_equal((ty1, t1a, t1b), (ty2, t2a, t2b), env)
        }

        // Wrapper types
        (Type::Ptr(i1), Type::Ptr(i2)) | (Type::Ref(i1), Type::Ref(i2)) => {
            types_equal_with_env(i1, i2, env)
        }

        // Type applications
        (Type::App(name1, args1), Type::App(name2, args2)) => {
            name1 == name2 && all_types_equal(args1, args2, env)
        }

        // ADT types
        (Type::Adt(name1, args1, vars1), Type::Adt(name2, args2, vars2)) => {
            adts_equal((name1, args1, vars1), (name2, args2, vars2), env)
        }

        // TypeError (poison) unifies with any type — suppress cascading errors
        (Type::Error, _) | (_, Type::Error) => true,

        _ => false,
    }
}

/// Check if two Eq types are equal: same underlying type + same term components.
fn eq_types_equal(
    eq1: (&Type, &crate::terms::Term, &crate::terms::Term),
    eq2: (&Type, &crate::terms::Term, &crate::terms::Term),
    env: &mut HashMap<String, String>,
) -> bool {
    types_equal_with_env(eq1.0, eq2.0, env) && eq1.1 == eq2.1 && eq1.2 == eq2.2
}

/// Check if two type variables are equal, respecting @-prefix stripping and env bindings.
fn tyvars_equal(v1: &str, v2: &str, env: &HashMap<String, String>) -> bool {
    let v1 = strip_named(v1);
    let v2 = strip_named(v2);
    env.get(v1)
        .map_or(v1 == v2, |mapped| strip_named(mapped) == v2)
}

/// Check if a TyVar equals a 0-arity App (which is semantically equivalent to a TyVar).
fn tyvar_app_equal(v: &str, name: &str, args: &[Type], env: &HashMap<String, String>) -> bool {
    if !args.is_empty() {
        return false;
    }
    let v = strip_named(v);
    env.get(v)
        .map_or(v == name, |mapped| strip_named(mapped) == name)
}

/// Check if two ADT types are structurally equal.
fn adts_equal(
    adt1: (&str, &[Type], &[(String, Type)]),
    adt2: (&str, &[Type], &[(String, Type)]),
    env: &mut HashMap<String, String>,
) -> bool {
    adt1.0 == adt2.0
        && adt1.1.len() == adt2.1.len()
        && adt1.2.len() == adt2.2.len()
        && all_types_equal(adt1.1, adt2.1, env)
        && adt1
            .2
            .iter()
            .zip(adt2.2.iter())
            .all(|((ctor1, pay1), (ctor2, pay2))| {
                ctor1 == ctor2 && types_equal_with_env(pay1, pay2, env)
            })
}

/// Temporarily bind `v1 → v2` in env, run `f`, then restore.
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

/// Check that two type-arg slices are pairwise equal.
fn all_types_equal(a: &[Type], b: &[Type], env: &mut HashMap<String, String>) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| types_equal_with_env(x, y, env))
}

#[cfg(test)]
mod equality_tests;
