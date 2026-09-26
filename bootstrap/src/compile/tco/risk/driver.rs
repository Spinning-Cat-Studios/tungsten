//! Recursion-driver classification: how a self-recursive function's parameter
//! types bound its stack depth (ADR 1.7.26b §2.1).
//!
//! Split out of `risk/mod.rs`, which aggregates decision *sites* into rows.
//! This half never looks at a [`Decision`] site — it answers only "given this
//! function's parameters, what bounds its recursion?", from the elaborated
//! parameter types (the same signal `audit-recursion`'s decompose hint reads),
//! grounded rather than a new traversal analysis.

use tungsten_codegen::Decision;
use tungsten_core::types::Type;

use super::Risk;

/// Rank a function's O(N)-stack risk (ADR 1.7.26b §2.1).
pub(super) fn classify_risk(decision: Decision, params: &[Type]) -> Risk {
    if decision.is_constant_stack() {
        return Risk::Low;
    }
    if params.iter().any(is_collection_driver) {
        Risk::High
    } else if params.iter().any(is_bounded_driver) {
        Risk::Med
    } else {
        Risk::Unknown
    }
}

/// Whether a parameter type is a collection / unbounded recursion driver:
/// recursive ADT (`Mu` — `List`, `Tree`, …), `String`, `Nat` source-position
/// index, or a known collection type application (`List`/`Array`/`Vec`).
fn is_collection_driver(ty: &Type) -> bool {
    match ty {
        Type::Mu(_, _) | Type::String | Type::Nat => true,
        Type::App(name, _) => matches!(name.as_str(), "List" | "Array" | "Vec" | "String"),
        _ => false,
    }
}

/// Whether a parameter is a structurally bounded finite driver: a non-recursive
/// named ADT (`Adt`), a generic type application (`App`), or a `Sum`/`Product`
/// aggregate. Recursive ADTs (`Mu`) are handled by [`is_collection_driver`].
fn is_bounded_driver(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Sum(_, _) | Type::Product(_, _) | Type::App(_, _) | Type::Adt(_, _, _)
    )
}

/// Display the recursion driver: the first collection param if any, else the
/// first param, else `—`.
pub(super) fn driver_display(params: &[Type]) -> String {
    let pick = params
        .iter()
        .find(|p| is_collection_driver(p))
        .or_else(|| params.first());
    match pick {
        Some(ty) => tungsten_bootstrap::driver::format_type(ty),
        None => "—".to_string(),
    }
}

/// Peel `Arrow`/`Forall` wrappers to recover parameter types.
pub(super) fn collect_param_types(ty: &Type) -> Vec<Type> {
    let mut params = Vec::new();
    let mut current = match ty {
        Type::Forall(_, body) => body.as_ref(),
        other => other,
    };
    while let Type::Arrow(param, ret) = current {
        params.push((**param).clone());
        current = ret;
    }
    params
}

/// Build a curried arrow type from parameter types and a result — shared with
/// `risk/mod.rs`'s aggregation tests, which need the same fixtures.
#[cfg(test)]
pub(super) fn arrow(params: Vec<Type>, ret: Type) -> Type {
    params
        .into_iter()
        .rev()
        .fold(ret, |acc, p| Type::Arrow(Box::new(p), Box::new(acc)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_param_skip_is_high() {
        let ty = arrow(vec![Type::App("List".into(), vec![Type::Nat])], Type::Nat);
        assert_eq!(
            classify_risk(Decision::Skip, &collect_param_types(&ty)),
            Risk::High
        );
    }

    #[test]
    fn nat_index_skip_is_high() {
        let ty = arrow(vec![Type::Nat], Type::Nat);
        assert_eq!(
            classify_risk(Decision::Skip, &collect_param_types(&ty)),
            Risk::High
        );
    }

    #[test]
    fn product_only_skip_is_med() {
        let ty = arrow(
            vec![Type::Product(Box::new(Type::Bool), Box::new(Type::Bool))],
            Type::Bool,
        );
        assert_eq!(
            classify_risk(Decision::Skip, &collect_param_types(&ty)),
            Risk::Med
        );
    }

    #[test]
    fn scalar_only_skip_is_unknown() {
        let ty = arrow(vec![Type::Bool], Type::Bool);
        assert_eq!(
            classify_risk(Decision::Skip, &collect_param_types(&ty)),
            Risk::Unknown
        );
    }

    #[test]
    fn emit_is_low_regardless_of_params() {
        let ty = arrow(vec![Type::App("List".into(), vec![Type::Nat])], Type::Nat);
        assert_eq!(
            classify_risk(Decision::Emit, &collect_param_types(&ty)),
            Risk::Low
        );
        assert_eq!(
            classify_risk(Decision::Decompose, &collect_param_types(&ty)),
            Risk::Low
        );
    }

    #[test]
    fn a_non_self_tail_edge_is_never_ranked_low() {
        // ADR 5.8.26a. SKIP_NON_SELF is not constant-stack, so if one ever
        // reached this classifier it would rank by driver like any other
        // non-EMIT decision. `risk/mod.rs` keeps them out of the inventory
        // entirely; this pins that the classifier does not quietly launder one
        // into a reassuring LOW.
        let ty = arrow(vec![Type::App("List".into(), vec![Type::Nat])], Type::Nat);
        assert_eq!(
            classify_risk(Decision::SkipNonSelf, &collect_param_types(&ty)),
            Risk::High
        );
    }

    #[test]
    fn driver_display_prefers_a_collection_param() {
        let ty = arrow(
            vec![Type::Bool, Type::App("List".into(), vec![Type::Nat])],
            Type::Nat,
        );
        assert_eq!(driver_display(&collect_param_types(&ty)), "List<Nat>");
        assert_eq!(driver_display(&[]), "—");
    }

    #[test]
    fn collect_param_types_peels_forall_and_arrows() {
        let inner = arrow(vec![Type::Bool, Type::Nat], Type::Nat);
        let ty = Type::Forall("a".to_string(), Box::new(inner));
        assert_eq!(collect_param_types(&ty), vec![Type::Bool, Type::Nat]);
    }
}
