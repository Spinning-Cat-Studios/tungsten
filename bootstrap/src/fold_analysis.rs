//! Shared fold/unfold analysis helpers for doctor and info commands.
//!
//! Provides structural Term traversal and μ-variable analysis used by
//! `doctor check-fold-consistency` and `info adt --check-fold`.

use tungsten_core::{Term, Type};

/// Count Fold or Unfold sites in a term tree that reference a given μ-variable.
pub fn count_in_term(term: &Term, mu_var: &str, is_fold: bool) -> usize {
    let this = match term {
        Term::Fold(ty, _) if is_fold && type_has_mu_var(ty, mu_var) => 1,
        Term::Unfold(ty, _) if !is_fold && type_has_mu_var(ty, mu_var) => 1,
        _ => 0,
    };
    let mut child_count = 0;
    term.for_each_subterm(|c| {
        child_count += count_in_term(c, mu_var, is_fold);
    });
    this + child_count
}

/// Count Fold or Unfold sites across all definitions.
pub fn count_fold_unfold_sites(
    defs: &[crate::elaborate::CoreDef],
    mu_var: &str,
    is_fold: bool,
) -> usize {
    defs.iter()
        .map(|def| count_in_term(&def.term.term, mu_var, is_fold))
        .sum()
}

/// Check if an encoded type's root is `Mu(α_<name>, _)`.
pub fn encoding_has_mu_binder(ty: &Type, name: &str) -> bool {
    let expected_var = format!("α_{name}");
    matches!(ty, Type::Mu(var, _) if var == &expected_var)
}

/// Check if a type mentions a specific μ-variable.
pub fn type_has_mu_var(ty: &Type, mu_var: &str) -> bool {
    match ty {
        Type::Mu(var, body) => var == mu_var || type_has_mu_var(body, mu_var),
        Type::TyVar(v) => v == mu_var,
        Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
            type_has_mu_var(a, mu_var) || type_has_mu_var(b, mu_var)
        }
        Type::Forall(_, body) | Type::Ptr(body) | Type::Ref(body) => type_has_mu_var(body, mu_var),
        Type::Eq(t, _, _) => type_has_mu_var(t, mu_var),
        Type::App(_, args) => args.iter().any(|a| type_has_mu_var(a, mu_var)),
        Type::Adt(_, type_args, variants) => {
            type_args.iter().any(|a| type_has_mu_var(a, mu_var))
                || variants.iter().any(|(_, vty)| type_has_mu_var(vty, mu_var))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────
    // count_in_term
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn count_fold_in_leaf() {
        assert_eq!(count_in_term(&Term::Zero, "α_List", true), 0);
    }

    #[test]
    fn count_fold_matching() {
        let ty = Type::Mu("α_List".into(), Box::new(Type::Nat));
        let term = Term::Fold(ty, Box::new(Term::Zero));
        assert_eq!(count_in_term(&term, "α_List", true), 1);
        assert_eq!(count_in_term(&term, "α_List", false), 0); // not unfold
    }

    #[test]
    fn count_unfold_matching() {
        let ty = Type::TyVar("α_List".into());
        let term = Term::Unfold(ty, Box::new(Term::Zero));
        assert_eq!(count_in_term(&term, "α_List", false), 1);
        assert_eq!(count_in_term(&term, "α_List", true), 0); // not fold
    }

    // ─────────────────────────────────────────────────────────────────
    // encoding_has_mu_binder
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn mu_binder_matching() {
        let ty = Type::Mu("α_List".into(), Box::new(Type::Nat));
        assert!(encoding_has_mu_binder(&ty, "List"));
    }

    #[test]
    fn mu_binder_wrong_name() {
        let ty = Type::Mu("α_Tree".into(), Box::new(Type::Nat));
        assert!(!encoding_has_mu_binder(&ty, "List"));
    }

    #[test]
    fn mu_binder_non_mu() {
        assert!(!encoding_has_mu_binder(&Type::Nat, "List"));
    }

    // ─────────────────────────────────────────────────────────────────
    // type_has_mu_var
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn mu_var_direct() {
        assert!(type_has_mu_var(&Type::TyVar("α_List".into()), "α_List"));
        assert!(!type_has_mu_var(&Type::TyVar("α_List".into()), "α_Tree"));
    }

    #[test]
    fn mu_var_nested_arrow() {
        let ty = Type::Arrow(Box::new(Type::Nat), Box::new(Type::TyVar("α_List".into())));
        assert!(type_has_mu_var(&ty, "α_List"));
    }

    #[test]
    fn mu_var_absent() {
        assert!(!type_has_mu_var(&Type::Nat, "α_List"));
        assert!(!type_has_mu_var(&Type::Bool, "α_List"));
    }

    #[test]
    fn mu_var_in_mu_binding_var() {
        // Mu(α_List, Nat) — the binder itself matches
        let ty = Type::Mu("α_List".into(), Box::new(Type::Nat));
        assert!(type_has_mu_var(&ty, "α_List"));
    }

    #[test]
    fn mu_var_in_mu_body() {
        // Mu(α_Other, Arrow(Nat, TyVar(α_List))) — found in body
        let ty = Type::Mu(
            "α_Other".into(),
            Box::new(Type::Arrow(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_List".into())),
            )),
        );
        assert!(type_has_mu_var(&ty, "α_List"));
        assert!(type_has_mu_var(&ty, "α_Other"));
    }

    #[test]
    fn mu_var_in_product() {
        let ty = Type::Product(Box::new(Type::TyVar("α_A".into())), Box::new(Type::Nat));
        assert!(type_has_mu_var(&ty, "α_A"));
        assert!(!type_has_mu_var(&ty, "α_B"));
    }

    #[test]
    fn mu_var_in_sum() {
        let ty = Type::Sum(Box::new(Type::Nat), Box::new(Type::TyVar("α_X".into())));
        assert!(type_has_mu_var(&ty, "α_X"));
        assert!(!type_has_mu_var(&ty, "α_Y"));
    }

    #[test]
    fn mu_var_in_app() {
        let ty = Type::App("List".into(), vec![Type::TyVar("α_T".into()), Type::Nat]);
        assert!(type_has_mu_var(&ty, "α_T"));
        assert!(!type_has_mu_var(&ty, "α_Z"));
    }

    #[test]
    fn mu_var_in_adt_type_args() {
        let ty = Type::Adt(
            "MyAdt".into(),
            vec![Type::TyVar("α_A".into())],
            vec![("V".into(), Type::Nat)],
        );
        assert!(type_has_mu_var(&ty, "α_A"));
    }

    #[test]
    fn mu_var_in_adt_variants() {
        let ty = Type::Adt(
            "MyAdt".into(),
            vec![Type::Nat],
            vec![("V".into(), Type::TyVar("α_B".into()))],
        );
        assert!(type_has_mu_var(&ty, "α_B"));
        assert!(!type_has_mu_var(&ty, "α_C"));
    }

    #[test]
    fn mu_var_in_eq() {
        let ty = Type::Eq(
            Box::new(Type::TyVar("α_X".into())),
            Box::new(Term::Zero),
            Box::new(Term::Zero),
        );
        assert!(type_has_mu_var(&ty, "α_X"));
        assert!(!type_has_mu_var(&ty, "α_Y"));
    }

    #[test]
    fn mu_var_in_forall() {
        let ty = Type::Forall("a".into(), Box::new(Type::TyVar("α_List".into())));
        assert!(type_has_mu_var(&ty, "α_List"));
        assert!(!type_has_mu_var(&ty, "α_Tree"));
    }

    // ─────────────────────────────────────────────────────────────────
    // count_fold_unfold_sites (direct unit test with hand-built CoreDefs)
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn count_fold_unfold_sites_empty_defs() {
        let defs: Vec<crate::elaborate::CoreDef> = vec![];
        assert_eq!(count_fold_unfold_sites(&defs, "α_List", true), 0);
    }

    #[test]
    fn count_fold_unfold_sites_sums_across_defs() {
        use tungsten_core::terms::{SpannedTerm, TermSpan};
        let mu_ty = Type::Mu("α_List".into(), Box::new(Type::Nat));
        let make_def = |term: Term| crate::elaborate::CoreDef {
            name: "f".into(),
            ty: Type::Nat,
            term: SpannedTerm {
                term,
                span: Some(TermSpan::new(0, 0)),
            },
            span: crate::span::Span { start: 0, end: 0 },
        };
        let defs = vec![
            make_def(Term::Fold(mu_ty.clone(), Box::new(Term::Zero))),
            make_def(Term::Fold(
                mu_ty.clone(),
                Box::new(Term::Fold(mu_ty, Box::new(Term::Zero))),
            )),
        ];
        // def 0: 1 fold, def 1: 2 folds = 3 total
        assert_eq!(count_fold_unfold_sites(&defs, "α_List", true), 3);
        assert_eq!(count_fold_unfold_sites(&defs, "α_List", false), 0);
    }
}
