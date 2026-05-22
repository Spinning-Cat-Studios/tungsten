//! Helper functions for direct call analysis and term decomposition.

use tungsten_core::terms::Term;
use tungsten_core::types::Type;

/// The `$direct` suffix appended to function names for uncurried entry points.
pub(crate) const DIRECT_SUFFIX: &str = "$direct";

/// Compute the source-level arity of a type (number of arrows).
pub(crate) fn type_arity(ty: &Type) -> usize {
    match ty {
        Type::Arrow(_, ret) => 1 + type_arity(ret),
        _ => 0,
    }
}

/// Build the direct entry point name for a function.
pub(crate) fn direct_name(base: &str) -> String {
    format!("{base}{DIRECT_SUFFIX}")
}

/// Collect the chain of parameters and return type from an Arrow type.
pub(super) fn collect_arrow_params(ty: &Type) -> (Vec<&Type>, &Type) {
    let mut params = Vec::new();
    let mut current = ty;
    while let Type::Arrow(param, ret) = current {
        params.push(param.as_ref());
        current = ret.as_ref();
    }
    (params, current)
}

/// Try to decompose a nested `App(App(App(Global(name), a1), a2), a3)`
/// into `Some((name, [a1, a2, a3]))`.
///
/// Returns `None` if the innermost callee is not a `Global`.
pub(crate) fn collect_saturated_call(term: &Term) -> Option<(String, Vec<&Term>)> {
    let mut args = Vec::new();
    let mut current = term;
    loop {
        match current {
            Term::App(func, arg) => {
                args.push(arg.as_ref());
                current = func.as_ref();
            }
            Term::Global(name) => {
                args.reverse();
                return Some((name.clone(), args));
            }
            // Transparent wrappers
            Term::Spanned(inner, _) | Term::Annot(inner, _) => {
                current = inner.as_ref();
            }
            _ => return None,
        }
    }
}

/// Unwrap `arity` layers of Lambda from a term, returning param names and the body.
///
/// Skips `Spanned` wrappers transparently.
pub(super) fn unwrap_lambda_chain(term: &Term, arity: usize) -> (Vec<String>, &Term) {
    let mut names = Vec::with_capacity(arity);
    let mut current = term;
    for _ in 0..arity {
        match current {
            Term::Lambda(x, _, body) => {
                names.push(x.clone());
                current = body.as_ref();
            }
            Term::Spanned(inner, _) | Term::Annot(inner, _) => {
                // Re-try after unwrapping
                return unwrap_lambda_chain_inner(inner, arity, names);
            }
            _ => break,
        }
    }
    (names, current)
}

fn unwrap_lambda_chain_inner(
    term: &Term,
    remaining: usize,
    mut names: Vec<String>,
) -> (Vec<String>, &Term) {
    let mut current = term;
    let collected = names.len();
    for _ in collected..remaining {
        match current {
            Term::Lambda(x, _, body) => {
                names.push(x.clone());
                current = body.as_ref();
            }
            Term::Spanned(inner, _) | Term::Annot(inner, _) => {
                current = inner.as_ref();
            }
            _ => break,
        }
    }
    (names, current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_core::types::Type;

    #[test]
    fn test_type_arity_zero() {
        assert_eq!(type_arity(&Type::Nat), 0);
    }

    #[test]
    fn test_type_arity_one() {
        let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
        assert_eq!(type_arity(&ty), 1);
    }

    #[test]
    fn test_type_arity_three() {
        let ty = Type::Arrow(
            Box::new(Type::Nat),
            Box::new(Type::Arrow(
                Box::new(Type::Bool),
                Box::new(Type::Arrow(Box::new(Type::String), Box::new(Type::Unit))),
            )),
        );
        assert_eq!(type_arity(&ty), 3);
    }

    #[test]
    fn test_direct_name() {
        assert_eq!(direct_name("foo"), "foo$direct");
        assert_eq!(direct_name("tungsten_main"), "tungsten_main$direct");
    }

    #[test]
    fn test_collect_saturated_call_single_app() {
        let term = Term::App(
            Box::new(Term::Global("f".to_string())),
            Box::new(Term::NatLit(42)),
        );
        let result = collect_saturated_call(&term);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "f");
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_collect_saturated_call_triple_app() {
        let term = Term::App(
            Box::new(Term::App(
                Box::new(Term::App(
                    Box::new(Term::Global("g".to_string())),
                    Box::new(Term::NatLit(1)),
                )),
                Box::new(Term::NatLit(2)),
            )),
            Box::new(Term::NatLit(3)),
        );
        let result = collect_saturated_call(&term);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "g");
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn test_collect_saturated_call_non_global() {
        let term = Term::App(
            Box::new(Term::Var("x".to_string())),
            Box::new(Term::NatLit(1)),
        );
        assert!(collect_saturated_call(&term).is_none());
    }

    #[test]
    fn test_unwrap_lambda_chain_basic() {
        let body = Term::NatLit(42);
        let term = Term::Lambda(
            "a".to_string(),
            Type::Nat,
            Box::new(Term::Lambda(
                "b".to_string(),
                Type::Bool,
                Box::new(body.clone()),
            )),
        );
        let (names, inner) = unwrap_lambda_chain(&term, 2);
        assert_eq!(names, vec!["a", "b"]);
        assert_eq!(*inner, body);
    }

    #[test]
    fn test_unwrap_lambda_chain_with_spanned() {
        let body = Term::NatLit(42);
        let inner_lambda = Term::Lambda("b".to_string(), Type::Bool, Box::new(body.clone()));
        let spanned = Term::Spanned(
            Box::new(inner_lambda),
            tungsten_core::terms::TermSpan::new(0, 10),
        );
        let term = Term::Lambda("a".to_string(), Type::Nat, Box::new(spanned));
        let (names, inner) = unwrap_lambda_chain(&term, 2);
        assert_eq!(names, vec!["a", "b"]);
        assert_eq!(*inner, body);
    }
}
