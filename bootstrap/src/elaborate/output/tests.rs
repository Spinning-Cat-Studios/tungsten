//! Tests for CoreDef boundary operations.

#[cfg(test)]
mod tests {
    use crate::elaborate::CoreDef;
    use crate::span::Span;
    use tungsten_core::terms::SpannedTerm;
    use tungsten_core::{Term, Type};

    /// Helper: create a CoreDef with the given type and term.
    fn make_def(ty: Type, term: Term) -> CoreDef {
        CoreDef {
            name: "test".to_string(),
            ty,
            term: SpannedTerm::generated(term),
            span: Span::empty(0),
        }
    }

    #[test]
    fn strip_at_prefixes_type_only() {
        // @-prefixed TyVar in the type signature
        let def = make_def(
            Type::arrow(Type::TyVar("@Token".into()), Type::Nat),
            Term::Zero,
        );
        let stripped = def.strip_at_prefixes();
        assert_eq!(
            stripped.ty,
            Type::arrow(Type::TyVar("Token".into()), Type::Nat)
        );
    }

    #[test]
    fn strip_at_prefixes_term_only() {
        // @-prefixed TyVar in a type annotation inside the term
        let def = make_def(
            Type::Nat,
            Term::Lambda(
                "x".into(),
                Type::TyVar("@Token".into()),
                Box::new(Term::Var("x".into())),
            ),
        );
        let stripped = def.strip_at_prefixes();
        assert_eq!(stripped.ty, Type::Nat);
        match &stripped.term.term {
            Term::Lambda(_, ty, _) => {
                assert_eq!(ty, &Type::TyVar("Token".into()));
            }
            other => panic!("expected Lambda, got {other:?}"),
        }
    }

    #[test]
    fn strip_at_prefixes_both() {
        // @-prefixed TyVars in both type and term
        let def = make_def(
            Type::TyVar("@List".into()),
            Term::annot(Term::Zero, Type::TyVar("@List".into())),
        );
        let stripped = def.strip_at_prefixes();
        assert_eq!(stripped.ty, Type::TyVar("List".into()));
        match &stripped.term.term {
            Term::Annot(_, ty) => {
                assert_eq!(ty, &Type::TyVar("List".into()));
            }
            other => panic!("expected Annot, got {other:?}"),
        }
    }

    #[test]
    fn strip_at_prefixes_no_op_without_at() {
        // No @-prefixed TyVars — should be identity
        let def = make_def(
            Type::arrow(Type::Nat, Type::Bool),
            Term::Lambda("x".into(), Type::Nat, Box::new(Term::True)),
        );
        let original_ty = def.ty.clone();
        let stripped = def.strip_at_prefixes();
        assert_eq!(stripped.ty, original_ty);
    }

    #[test]
    fn strip_at_prefixes_preserves_alpha_prefix() {
        // α_-prefixed TyVars (Mu-bound) should NOT be stripped
        let def = make_def(
            Type::Mu("α_List".into(), Box::new(Type::TyVar("α_List".into()))),
            Term::Zero,
        );
        let stripped = def.strip_at_prefixes();
        assert_eq!(
            stripped.ty,
            Type::Mu("α_List".into(), Box::new(Type::TyVar("α_List".into())))
        );
    }

    #[test]
    fn strip_at_prefixes_nested_term_types() {
        // @-prefixed TyVars nested deep in the term tree:
        // Lambda wrapping Let wrapping TyApp — all should be stripped.
        let inner = Term::ty_app(Term::Global("f".into()), Type::TyVar("@Token".into()));
        let let_body = Term::Let(
            "y".into(),
            Type::TyVar("@List".into()),
            Box::new(inner),
            Box::new(Term::Var("y".into())),
        );
        let term = Term::Lambda("x".into(), Type::TyVar("@Token".into()), Box::new(let_body));
        let def = make_def(Type::Nat, term);
        let stripped = def.strip_at_prefixes();

        // Verify Lambda's annotation stripped
        let Term::Lambda(_, ty, body) = &stripped.term.term else {
            panic!("expected Lambda, got {:?}", stripped.term.term);
        };
        assert_eq!(ty, &Type::TyVar("Token".into()));
        // Verify Let's annotation stripped
        let Term::Let(_, let_ty, val, _) = body.as_ref() else {
            panic!("expected Let, got {:?}", body);
        };
        assert_eq!(let_ty, &Type::TyVar("List".into()));
        // Verify TyApp's type arg stripped
        let Term::TyApp(_, ty_arg) = val.as_ref() else {
            panic!("expected TyApp, got {:?}", val);
        };
        assert_eq!(ty_arg, &Type::TyVar("Token".into()));
    }

    #[test]
    fn strip_at_prefixes_multiple_at_vars() {
        // Multiple distinct @-prefixed TyVars — all should be stripped
        let def = make_def(
            Type::arrow(Type::TyVar("@Token".into()), Type::TyVar("@List".into())),
            Term::Lambda(
                "x".into(),
                Type::TyVar("@Token".into()),
                Box::new(Term::annot(
                    Term::Var("x".into()),
                    Type::TyVar("@List".into()),
                )),
            ),
        );
        let stripped = def.strip_at_prefixes();
        assert_eq!(
            stripped.ty,
            Type::arrow(Type::TyVar("Token".into()), Type::TyVar("List".into()))
        );
        let Term::Lambda(_, ty, body) = &stripped.term.term else {
            panic!("expected Lambda, got {:?}", stripped.term.term);
        };
        assert_eq!(ty, &Type::TyVar("Token".into()));
        let Term::Annot(_, ann_ty) = body.as_ref() else {
            panic!("expected Annot, got {:?}", body);
        };
        assert_eq!(ann_ty, &Type::TyVar("List".into()));
    }
}
