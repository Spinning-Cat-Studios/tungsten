//! Utility functions for collecting TyVar information from type trees.
//!
//! Used by phase invariant checks to detect @-prefixed references
//! and unbound type variables in cached encodings.

use tungsten_core::Type;

/// Collect all @-prefixed TyVar names found in a type tree.
///
/// Only the `@`-prefixed `TyVar` leaf is non-uniform; every other variant
/// (including a bare `TyVar`, whose `children()` are empty) delegates to
/// [`Type::children`] (ADR 23.7.26b).
pub(super) fn collect_at_prefixed_tyvars(ty: &Type, results: &mut Vec<String>) {
    match ty {
        Type::TyVar(name) if name.starts_with('@') => {
            results.push(name.clone());
        }
        _ => {
            for child in ty.children() {
                collect_at_prefixed_tyvars(child, results);
            }
        }
    }
}

/// Check if a type contains any TyVar escapes (TyVars that are not
/// μ-binder variables). A TyVar starting with "α_" is a μ-binder variable
/// and is expected. Other TyVars in a cached encoding are suspicious.
/// Non-uniform arms: the `TyVar` leaf (checked against `bound`) and the two
/// binders (`Mu`/`Forall`), which push/pop their bound name around the body
/// walk. Every other variant delegates to [`Type::children`] (ADR 23.7.26b).
pub(super) fn collect_non_mu_tyvars(ty: &Type, bound: &mut Vec<String>, results: &mut Vec<String>) {
    match ty {
        Type::TyVar(name) => {
            if !bound.contains(name) && !name.starts_with('@') {
                results.push(name.clone());
            }
        }
        Type::Mu(binder, body) | Type::Forall(binder, body) => {
            bound.push(binder.clone());
            collect_non_mu_tyvars(body, bound, results);
            bound.pop();
        }
        _ => {
            for child in ty.children() {
                collect_non_mu_tyvars(child, bound, results);
            }
        }
    }
}
