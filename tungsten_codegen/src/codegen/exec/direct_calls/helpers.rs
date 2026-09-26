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

/// Try to decompose a saturated call whose head may carry type applications:
/// `App*(TyApp*(Global(name), T1..), a1..)` into `Some((name, [T1..], [a1..]))`.
///
/// A monomorphic call yields an empty type-arg list. Returns `None` if the
/// innermost callee is not a `Global`, or if a `TyApp` layer appears above an
/// `App` layer (not a first-order call shape). See ADR 23.7.26c: the type-arg
/// form lets saturated generic calls resolve their monomorphized instance and
/// ride the `$direct` path instead of the curried closure-wrapper chain.
pub(crate) fn collect_saturated_generic_call(
    term: &Term,
) -> Option<(String, Vec<&Type>, Vec<&Term>)> {
    let mut args = Vec::new();
    let mut current = term;
    // Peel value applications.
    loop {
        match current {
            Term::App(func, arg) => {
                args.push(arg.as_ref());
                current = func.as_ref();
            }
            Term::Spanned(inner, _) | Term::Annot(inner, _) => {
                current = inner.as_ref();
            }
            _ => break,
        }
    }
    // Peel type applications (the head of a generic call).
    let mut ty_args = Vec::new();
    loop {
        match current {
            Term::TyApp(func, ty_arg) => {
                ty_args.push(ty_arg);
                current = func.as_ref();
            }
            Term::Spanned(inner, _) | Term::Annot(inner, _) => {
                current = inner.as_ref();
            }
            Term::Global(name) => {
                args.reverse();
                ty_args.reverse();
                return Some((name.clone(), ty_args, args));
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
        let result = collect_saturated_generic_call(&term);
        assert!(result.is_some());
        let (name, ty_args, args) = result.unwrap();
        assert_eq!(name, "f");
        assert!(ty_args.is_empty());
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
        let result = collect_saturated_generic_call(&term);
        assert!(result.is_some());
        let (name, ty_args, args) = result.unwrap();
        assert_eq!(name, "g");
        assert!(ty_args.is_empty());
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn test_collect_saturated_call_non_global() {
        let term = Term::App(
            Box::new(Term::Var("x".to_string())),
            Box::new(Term::NatLit(1)),
        );
        assert!(collect_saturated_generic_call(&term).is_none());
    }

    #[test]
    fn test_collect_saturated_generic_call_tyapp_head() {
        // spin_generic<Nat>(1, 2, 3) — App(App(App(TyApp(Global, Nat), 1), 2), 3)
        let head = Term::TyApp(
            Box::new(Term::Global("spin_generic".to_string())),
            Type::Nat,
        );
        let term = Term::App(
            Box::new(Term::App(
                Box::new(Term::App(Box::new(head), Box::new(Term::NatLit(1)))),
                Box::new(Term::NatLit(2)),
            )),
            Box::new(Term::NatLit(3)),
        );
        let (name, ty_args, args) = collect_saturated_generic_call(&term).unwrap();
        assert_eq!(name, "spin_generic");
        assert_eq!(ty_args.len(), 1);
        assert_eq!(*ty_args[0], Type::Nat);
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn test_collect_saturated_generic_call_multi_tyapp_order() {
        // pmap<A, B>(x) — TyApps nest innermost-first; collected order must be [A, B]
        let head = Term::TyApp(
            Box::new(Term::TyApp(
                Box::new(Term::Global("pmap".to_string())),
                Type::Nat,
            )),
            Type::Bool,
        );
        let term = Term::App(Box::new(head), Box::new(Term::NatLit(1)));
        let (name, ty_args, args) = collect_saturated_generic_call(&term).unwrap();
        assert_eq!(name, "pmap");
        assert_eq!(ty_args.len(), 2);
        assert_eq!(*ty_args[0], Type::Nat);
        assert_eq!(*ty_args[1], Type::Bool);
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_collect_saturated_generic_call_tyapp_over_non_global() {
        // TyApp over a local (TyAbs) head is not a first-order instance call.
        let head = Term::TyApp(Box::new(Term::Var("local_fn".to_string())), Type::Nat);
        let term = Term::App(Box::new(head), Box::new(Term::NatLit(1)));
        assert!(collect_saturated_generic_call(&term).is_none());
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

// ── DirectCallState accessors (ADR 2.7.26b T7) ──────────────────────────────
// The struct lives in `codegen/mod.rs` with the other concern sub-structs;
// its per-entry accessors live here with their consumers.

impl<'ctx> crate::codegen::DirectCallState<'ctx> {
    pub(crate) fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            current_entry: None,
        }
    }

    pub(crate) fn set_arity(&mut self, name: &str, arity: usize) {
        self.entries.entry(name.to_string()).or_default().arity = Some(arity);
    }

    pub(crate) fn arity(&self, name: &str) -> Option<usize> {
        self.entries.get(name).and_then(|e| e.arity)
    }

    pub(crate) fn set_decompose_map(
        &mut self,
        name: &str,
        map: Vec<crate::codegen::exec::direct_calls::decompose::ParamLowering>,
    ) {
        self.entries
            .entry(name.to_string())
            .or_default()
            .decompose_map = Some(map);
    }

    pub(crate) fn decompose_map(
        &self,
        name: &str,
    ) -> Option<&Vec<crate::codegen::exec::direct_calls::decompose::ParamLowering>> {
        self.entries
            .get(name)
            .and_then(|e| e.decompose_map.as_ref())
    }

    pub(crate) fn set_lowered_sig(
        &mut self,
        name: &str,
        sig: crate::codegen::abi::LoweredSignature<'ctx>,
    ) {
        self.entries
            .entry(name.to_string())
            .or_default()
            .lowered_sig = Some(sig);
    }

    pub(crate) fn lowered_sig(
        &self,
        name: &str,
    ) -> Option<&crate::codegen::abi::LoweredSignature<'ctx>> {
        self.entries.get(name).and_then(|e| e.lowered_sig.as_ref())
    }
}
