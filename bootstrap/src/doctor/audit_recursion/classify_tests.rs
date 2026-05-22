//! Tests for recursion classification.

#[cfg(test)]
mod tests {
    use crate::doctor::audit_recursion::classify::*;
    use tungsten_core::terms::Term;
    use tungsten_core::types::Type;

    #[test]
    fn test_tail_recursive() {
        // fix f. λn. λacc. if n == 0 then acc else f (n-1) (acc+1)
        let body = Term::If(
            Box::new(Term::NatEq(
                Box::new(Term::Var("n".to_string())),
                Box::new(Term::Zero),
            )),
            Box::new(Term::Var("acc".to_string())),
            Box::new(Term::App(
                Box::new(Term::App(
                    Box::new(Term::Var("f".to_string())),
                    Box::new(Term::NatSub(
                        Box::new(Term::Var("n".to_string())),
                        Box::new(Term::NatLit(1)),
                    )),
                )),
                Box::new(Term::NatAdd(
                    Box::new(Term::Var("acc".to_string())),
                    Box::new(Term::NatLit(1)),
                )),
            )),
        );
        let term = Term::Fix(
            "f".to_string(),
            Type::Nat,
            Box::new(Term::Lambda(
                "n".to_string(),
                Type::Nat,
                Box::new(Term::Lambda("acc".to_string(), Type::Nat, Box::new(body))),
            )),
        );

        assert_eq!(
            classify_recursion("my_fn", &term),
            RecursionKind::TailRecursive
        );
    }

    #[test]
    fn test_tree_recursive() {
        // fix f. λn. if n == 0 then 1 else f(n-1) + f(n-2)
        let body = Term::If(
            Box::new(Term::NatEq(
                Box::new(Term::Var("n".to_string())),
                Box::new(Term::Zero),
            )),
            Box::new(Term::NatLit(1)),
            Box::new(Term::NatAdd(
                Box::new(Term::App(
                    Box::new(Term::Var("f".to_string())),
                    Box::new(Term::NatSub(
                        Box::new(Term::Var("n".to_string())),
                        Box::new(Term::NatLit(1)),
                    )),
                )),
                Box::new(Term::App(
                    Box::new(Term::Var("f".to_string())),
                    Box::new(Term::NatSub(
                        Box::new(Term::Var("n".to_string())),
                        Box::new(Term::NatLit(2)),
                    )),
                )),
            )),
        );
        let term = Term::Fix(
            "f".to_string(),
            Type::Nat,
            Box::new(Term::Lambda("n".to_string(), Type::Nat, Box::new(body))),
        );

        assert_eq!(
            classify_recursion("fib", &term),
            RecursionKind::TreeRecursive
        );
    }

    #[test]
    fn test_linear_non_tail() {
        // fix f. λn. if n == 0 then 0 else 1 + f(n-1)
        let body = Term::If(
            Box::new(Term::NatEq(
                Box::new(Term::Var("n".to_string())),
                Box::new(Term::Zero),
            )),
            Box::new(Term::Zero),
            Box::new(Term::NatAdd(
                Box::new(Term::NatLit(1)),
                Box::new(Term::App(
                    Box::new(Term::Var("f".to_string())),
                    Box::new(Term::NatSub(
                        Box::new(Term::Var("n".to_string())),
                        Box::new(Term::NatLit(1)),
                    )),
                )),
            )),
        );
        let term = Term::Fix(
            "f".to_string(),
            Type::Nat,
            Box::new(Term::Lambda("n".to_string(), Type::Nat, Box::new(body))),
        );

        assert_eq!(
            classify_recursion("count", &term),
            RecursionKind::LinearNonTail
        );
    }
}
