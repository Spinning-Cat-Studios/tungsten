//! Musttail decomposition eligibility hints for `audit-recursion` (ADR 18.5.26a).
//!
//! Source-level classification of whether a tail-recursive function's struct
//! params could be flattened for musttail — shown as an annotation next to each
//! tail-recursive function. Distinct from the codegen `MusttailDecision` gate
//! (ADR 1.7.26b), which is authoritative when codegen is consulted.

use std::collections::HashMap;

use tungsten_core::types::Type;

/// Decomposition eligibility hint for musttail (ADR 18.5.26a).
#[derive(Debug)]
pub enum DecomposeHint {
    /// No struct-typed params — musttail works directly.
    NoStructParams,
    /// Has struct params that are decomposition-eligible. Count of struct params.
    Eligible(usize),
    /// Has struct params but not eligible for decomposition.
    Ineligible(&'static str),
    /// Not tail-recursive, so decomposition is irrelevant.
    NotTailRecursive,
}

/// Classify a tail-recursive function's decomposition eligibility based on Core IR types.
pub(super) fn classify_decompose_hint(
    name: &str,
    type_map: &HashMap<String, &Type>,
) -> DecomposeHint {
    let ty = match type_map.get(name) {
        Some(t) => t,
        None => return DecomposeHint::NoStructParams,
    };
    let params = collect_param_types(ty);
    if params.is_empty() {
        return DecomposeHint::NoStructParams;
    }
    let mut struct_count = 0;
    for param in &params {
        match param {
            // String lowers to { ptr, i64 } — flattenable
            Type::String => struct_count += 1,
            // Product lowers to struct — flattenable if fields are scalar
            Type::Product(_, _) => struct_count += 1,
            // Sum/Arrow lower to structs with nested struct/array fields
            Type::Sum(_, _) | Type::Arrow(_, _) => {
                return DecomposeHint::Ineligible(" [struct params, not flattenable]");
            }
            _ => {} // scalar or pointer — no struct issue
        }
    }
    if struct_count > 0 {
        DecomposeHint::Eligible(struct_count)
    } else {
        DecomposeHint::NoStructParams
    }
}

/// Extract parameter types from a function type (peeling Arrow wrappers).
fn collect_param_types(ty: &Type) -> Vec<Type> {
    let mut params = Vec::new();
    let mut current = ty;
    while let Type::Arrow(param, ret) = current {
        params.push((**param).clone());
        current = ret;
    }
    // Also handle Forall wrappers (polymorphic functions)
    if let Type::Forall(_, body) = ty {
        return collect_param_types(body);
    }
    params
}

#[cfg(test)]
mod decompose_hint_tests {
    use super::*;

    fn make_type_map(entries: Vec<(&str, Type)>) -> HashMap<String, Type> {
        entries
            .into_iter()
            .map(|(n, t)| (n.to_string(), t))
            .collect()
    }

    fn classify_with(name: &str, ty: Type) -> DecomposeHint {
        let map = make_type_map(vec![(name, ty)]);
        let ref_map: HashMap<String, &Type> = map.iter().map(|(k, v)| (k.clone(), v)).collect();
        classify_decompose_hint(name, &ref_map)
    }

    #[test]
    fn scalar_params_no_decompose() {
        // fn(Nat) -> Nat
        let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat));
        assert!(matches!(
            classify_with("f", ty),
            DecomposeHint::NoStructParams
        ));
    }

    #[test]
    fn string_param_is_eligible() {
        // fn(String) -> Nat
        let ty = Type::Arrow(Box::new(Type::String), Box::new(Type::Nat));
        assert!(matches!(classify_with("f", ty), DecomposeHint::Eligible(1)));
    }

    #[test]
    fn product_param_is_eligible() {
        // fn(Product) -> Nat
        let ty = Type::Arrow(
            Box::new(Type::Product(Box::new(Type::Nat), Box::new(Type::Nat))),
            Box::new(Type::Nat),
        );
        assert!(matches!(classify_with("f", ty), DecomposeHint::Eligible(1)));
    }

    #[test]
    fn sum_param_is_ineligible() {
        // fn(Sum) -> Nat — Sum lowers to struct+tag, not flattenable
        let ty = Type::Arrow(
            Box::new(Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool))),
            Box::new(Type::Nat),
        );
        assert!(matches!(
            classify_with("f", ty),
            DecomposeHint::Ineligible(_)
        ));
    }

    #[test]
    fn arrow_param_is_ineligible() {
        // fn(fn(Nat)->Nat) -> Nat — Arrow lowers to closure struct
        let inner = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat));
        let ty = Type::Arrow(Box::new(inner), Box::new(Type::Nat));
        assert!(matches!(
            classify_with("f", ty),
            DecomposeHint::Ineligible(_)
        ));
    }

    #[test]
    fn mixed_string_and_nat() {
        // fn(String, Nat, String) -> Nat — 2 struct params
        let ty = Type::Arrow(
            Box::new(Type::String),
            Box::new(Type::Arrow(
                Box::new(Type::Nat),
                Box::new(Type::Arrow(Box::new(Type::String), Box::new(Type::Nat))),
            )),
        );
        assert!(matches!(classify_with("f", ty), DecomposeHint::Eligible(2)));
    }

    #[test]
    fn unknown_function_returns_no_struct() {
        let map: HashMap<String, Type> = HashMap::new();
        let ref_map: HashMap<String, &Type> = map.iter().map(|(k, v)| (k.clone(), v)).collect();
        assert!(matches!(
            classify_decompose_hint("nonexistent", &ref_map),
            DecomposeHint::NoStructParams
        ));
    }
}
