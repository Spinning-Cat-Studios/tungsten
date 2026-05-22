//! Utility functions for collecting TyVar information from type trees.
//!
//! Used by phase invariant checks to detect @-prefixed references
//! and unbound type variables in cached encodings.

use tungsten_core::Type;

/// Collect all @-prefixed TyVar names found in a type tree.
pub(super) fn collect_at_prefixed_tyvars(ty: &Type, results: &mut Vec<String>) {
    match ty {
        Type::TyVar(name) if name.starts_with('@') => {
            results.push(name.clone());
        }
        Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
            collect_at_prefixed_tyvars(a, results);
            collect_at_prefixed_tyvars(b, results);
        }
        Type::Eq(ty, _, _) => {
            collect_at_prefixed_tyvars(ty, results);
        }
        Type::Mu(_, body) | Type::Ptr(body) | Type::Ref(body) => {
            collect_at_prefixed_tyvars(body, results);
        }
        Type::Forall(_, body) => {
            collect_at_prefixed_tyvars(body, results);
        }
        Type::App(_, args) => {
            for arg in args {
                collect_at_prefixed_tyvars(arg, results);
            }
        }
        Type::Adt(_, type_args, fields) => {
            for arg in type_args {
                collect_at_prefixed_tyvars(arg, results);
            }
            for (_, ty) in fields {
                collect_at_prefixed_tyvars(ty, results);
            }
        }
        // Leaf types: no children to recurse into
        Type::Nat
        | Type::Bool
        | Type::Unit
        | Type::Void
        | Type::Prop
        | Type::String
        | Type::TyVar(_)
        | Type::Error => {}
    }
}

/// Check if a type contains any TyVar escapes (TyVars that are not
/// μ-binder variables). A TyVar starting with "α_" is a μ-binder variable
/// and is expected. Other TyVars in a cached encoding are suspicious.
pub(super) fn collect_non_mu_tyvars(ty: &Type, bound: &mut Vec<String>, results: &mut Vec<String>) {
    match ty {
        Type::TyVar(name) => {
            if !bound.contains(name) && !name.starts_with('@') {
                results.push(name.clone());
            }
        }
        Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
            collect_non_mu_tyvars(a, bound, results);
            collect_non_mu_tyvars(b, bound, results);
        }
        Type::Eq(ty, _, _) => {
            collect_non_mu_tyvars(ty, bound, results);
        }
        Type::Mu(binder, body) => {
            bound.push(binder.clone());
            collect_non_mu_tyvars(body, bound, results);
            bound.pop();
        }
        Type::Ptr(body) | Type::Ref(body) => {
            collect_non_mu_tyvars(body, bound, results);
        }
        Type::Forall(param, body) => {
            bound.push(param.clone());
            collect_non_mu_tyvars(body, bound, results);
            bound.pop();
        }
        Type::App(_, args) => {
            for arg in args {
                collect_non_mu_tyvars(arg, bound, results);
            }
        }
        Type::Adt(_, type_args, fields) => {
            for arg in type_args {
                collect_non_mu_tyvars(arg, bound, results);
            }
            for (_, ty) in fields {
                collect_non_mu_tyvars(ty, bound, results);
            }
        }
        Type::Nat
        | Type::Bool
        | Type::Unit
        | Type::Void
        | Type::Prop
        | Type::String
        | Type::Error => {}
    }
}
