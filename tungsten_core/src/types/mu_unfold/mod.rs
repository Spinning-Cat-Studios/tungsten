//! Equi-recursive μ-type unfolding (ADR 7.7.26k).
//!
//! Replaces the accumulated-substitution unfold loop that materialized
//! exponentially-sized trees for nested μ-binder chains (the ADR 18.4.26i
//! mutual-recursion group encoding). Under that encoding each SCC member's
//! stored type wraps the member's *own* body in the whole group's binder
//! chain, so every chain variable denotes the same regular tree as the
//! whole μ-type — which means each variable can be replaced by the original
//! input itself, in one simultaneous pass, instead of by an accumulated
//! partially-substituted giant. Output size is linear in
//! (chain-variable occurrences × |input|) where the old loop was
//! exponential in the chain length (∏ kᵢ).

use super::Type;

/// Unfold a (possibly nested) μ-type one level.
///
/// For `μX₁…μXₙ. F`, returns `F` with every non-shadowed occurrence of
/// `X₁…Xₙ` replaced by the whole input μ-type. Non-μ inputs are returned
/// unchanged.
///
/// A vacuous body (`μX. X`) returns the input itself — still a `Mu` — where
/// the old accumulated loop diverged; callers treat a residual `Mu` like any
/// other non-structural type and report it at their own dispatch site.
#[must_use]
pub fn unfold_mu_type(ty: &Type) -> Type {
    let mut chain: Vec<&str> = Vec::new();
    let mut body: &Type = ty;
    while let Type::Mu(var, inner) = body {
        chain.push(var.as_str());
        body = inner;
    }
    if chain.is_empty() {
        return ty.clone();
    }
    substitute_chain_vars(body, &chain, ty)
}

/// Substitute every free occurrence of any variable in `chain` with
/// `replacement`, in one pass. Inner `Mu`/`Forall` binders shadow their
/// variable for the subtree they enclose.
fn substitute_chain_vars(ty: &Type, chain: &[&str], replacement: &Type) -> Type {
    match ty {
        Type::TyVar(v) if chain.contains(&v.as_str()) => replacement.clone(),
        Type::TyVar(_)
        | Type::Unit
        | Type::Bool
        | Type::Nat
        | Type::Int
        | Type::String
        | Type::Void
        | Type::Prop
        | Type::Error => ty.clone(),

        Type::Arrow(a, b) => Type::Arrow(
            Box::new(substitute_chain_vars(a, chain, replacement)),
            Box::new(substitute_chain_vars(b, chain, replacement)),
        ),
        Type::Product(a, b) => Type::Product(
            Box::new(substitute_chain_vars(a, chain, replacement)),
            Box::new(substitute_chain_vars(b, chain, replacement)),
        ),
        Type::Sum(a, b) => Type::Sum(
            Box::new(substitute_chain_vars(a, chain, replacement)),
            Box::new(substitute_chain_vars(b, chain, replacement)),
        ),

        Type::Forall(v, body) => Type::Forall(
            v.clone(),
            Box::new(substitute_under_binder(v, body, chain, replacement)),
        ),
        Type::Mu(v, body) => Type::Mu(
            v.clone(),
            Box::new(substitute_under_binder(v, body, chain, replacement)),
        ),

        // Only the type component participates in substitution; the term
        // components are opaque here (mirrors the previous codegen
        // substitute_type behaviour).
        Type::Eq(ty_inner, t1, t2) => Type::Eq(
            Box::new(substitute_chain_vars(ty_inner, chain, replacement)),
            t1.clone(),
            t2.clone(),
        ),

        Type::Ptr(inner) => Type::Ptr(Box::new(substitute_chain_vars(inner, chain, replacement))),
        Type::Ref(inner) => Type::Ref(Box::new(substitute_chain_vars(inner, chain, replacement))),

        Type::App(name, args) => Type::App(
            name.clone(),
            args.iter()
                .map(|a| substitute_chain_vars(a, chain, replacement))
                .collect(),
        ),

        Type::Adt(name, type_args, variants) => Type::Adt(
            name.clone(),
            type_args
                .iter()
                .map(|a| substitute_chain_vars(a, chain, replacement))
                .collect(),
            variants
                .iter()
                .map(|(vname, vty)| {
                    (
                        vname.clone(),
                        substitute_chain_vars(vty, chain, replacement),
                    )
                })
                .collect(),
        ),
    }
}

/// Recurse into a binder body with the binder's variable removed from the
/// active chain (shadowing). If nothing remains to substitute, the subtree
/// is cloned as-is.
fn substitute_under_binder(
    binder_var: &str,
    body: &Type,
    chain: &[&str],
    replacement: &Type,
) -> Type {
    if chain.contains(&binder_var) {
        let inner_chain: Vec<&str> = chain
            .iter()
            .copied()
            .filter(|chain_var| *chain_var != binder_var)
            .collect();
        if inner_chain.is_empty() {
            return body.clone();
        }
        substitute_chain_vars(body, &inner_chain, replacement)
    } else {
        substitute_chain_vars(body, chain, replacement)
    }
}

#[cfg(test)]
mod tests;
