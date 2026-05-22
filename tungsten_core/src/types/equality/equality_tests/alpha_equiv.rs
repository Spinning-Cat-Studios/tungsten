use super::*;

#[test]
fn test_mu_alpha_equivalence_same_var() {
    // μα. Unit + α  ≡  μα. Unit + α
    let ty1 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
    let ty2 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
    assert!(types_equal_alpha(&ty1, &ty2));
}

#[test]
fn test_mu_alpha_equivalence_different_var() {
    // μα. Unit + α  ≡  μβ. Unit + β
    let ty1 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
    let ty2 = Type::mu("beta", Type::sum(Type::Unit, Type::TyVar("beta".into())));
    assert!(types_equal_alpha(&ty1, &ty2));
}

#[test]
fn test_mu_not_equal_different_structure() {
    // μα. Unit + α  ≢  μα. Nat + α
    let ty1 = Type::mu("alpha", Type::sum(Type::Unit, Type::TyVar("alpha".into())));
    let ty2 = Type::mu("alpha", Type::sum(Type::Nat, Type::TyVar("alpha".into())));
    assert!(!types_equal_alpha(&ty1, &ty2));
}

#[test]
fn test_mu_nested_alpha_equivalence() {
    // μα. μβ. α × β  ≡  μx. μy. x × y
    let ty1 = Type::mu(
        "alpha",
        Type::mu(
            "beta",
            Type::product(Type::TyVar("alpha".into()), Type::TyVar("beta".into())),
        ),
    );
    let ty2 = Type::mu(
        "x",
        Type::mu(
            "y",
            Type::product(Type::TyVar("x".into()), Type::TyVar("y".into())),
        ),
    );
    assert!(types_equal_alpha(&ty1, &ty2));
}

#[test]
fn test_forall_alpha_equivalence() {
    // ∀α. α → α  ≡  ∀β. β → β
    let ty1 = Type::forall(
        "alpha",
        Type::arrow(Type::TyVar("alpha".into()), Type::TyVar("alpha".into())),
    );
    let ty2 = Type::forall(
        "beta",
        Type::arrow(Type::TyVar("beta".into()), Type::TyVar("beta".into())),
    );
    assert!(types_equal_alpha(&ty1, &ty2));
}

#[test]
fn test_free_vs_bound_not_equal() {
    // μα. α  ≢  μα. β  (where β is free)
    let ty1 = Type::mu("alpha", Type::TyVar("alpha".into()));
    let ty2 = Type::mu("alpha", Type::TyVar("beta".into()));
    assert!(!types_equal_alpha(&ty1, &ty2));
}

#[test]
fn test_base_types_equal() {
    assert!(types_equal_alpha(&Type::Nat, &Type::Nat));
    assert!(types_equal_alpha(&Type::Bool, &Type::Bool));
    assert!(types_equal_alpha(&Type::Unit, &Type::Unit));
    assert!(types_equal_alpha(&Type::String, &Type::String));
    assert!(!types_equal_alpha(&Type::Nat, &Type::Bool));
}

#[test]
fn test_complex_list_type_equivalence() {
    // List<Nat> representation: μα. Unit + (Nat × α)
    // With different bound var names should be equal
    let list1 = Type::mu(
        "α_List",
        Type::sum(
            Type::Unit,
            Type::product(Type::Nat, Type::TyVar("α_List".into())),
        ),
    );
    let list2 = Type::mu(
        "rec",
        Type::sum(
            Type::Unit,
            Type::product(Type::Nat, Type::TyVar("rec".into())),
        ),
    );
    assert!(types_equal_alpha(&list1, &list2));
}
