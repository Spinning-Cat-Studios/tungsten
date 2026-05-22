use super::*;

#[test]
fn test_tyvar_equals_mu_with_alpha_prefix() {
    // TyVar("TypeExpr") should equal Mu("α_TypeExpr", body)
    // This handles the normalization depth asymmetry where one side references
    // a type by name and the other has its recursive encoding.
    let ty1 = Type::TyVar("TypeExpr".into());
    let ty2 = Type::mu(
        "α_TypeExpr",
        Type::sum(Type::Unit, Type::TyVar("α_TypeExpr".into())),
    );
    assert!(
        types_equal_alpha(&ty1, &ty2),
        "TyVar(X) should equal Mu(α_X, body)"
    );
}

#[test]
fn test_mu_equals_tyvar_symmetric() {
    // Symmetric case: Mu("α_Stmt", body) should equal TyVar("Stmt")
    let ty1 = Type::mu(
        "α_Stmt",
        Type::sum(Type::Unit, Type::TyVar("α_Stmt".into())),
    );
    let ty2 = Type::TyVar("Stmt".into());
    assert!(
        types_equal_alpha(&ty1, &ty2),
        "Mu(α_X, body) should equal TyVar(X)"
    );
}

#[test]
fn test_tyvar_not_equal_mu_wrong_prefix() {
    // TyVar("Foo") should NOT equal Mu("β_Foo", body) - wrong prefix
    let ty1 = Type::TyVar("Foo".into());
    let ty2 = Type::mu("β_Foo", Type::sum(Type::Unit, Type::TyVar("β_Foo".into())));
    assert!(
        !types_equal_alpha(&ty1, &ty2),
        "TyVar(X) should not equal Mu(β_X, body) - only α_ prefix works"
    );
}

#[test]
fn test_tyvar_not_equal_mu_name_mismatch() {
    // TyVar("Foo") should NOT equal Mu("α_Bar", body) - different names
    let ty1 = Type::TyVar("Foo".into());
    let ty2 = Type::mu("α_Bar", Type::sum(Type::Unit, Type::TyVar("α_Bar".into())));
    assert!(
        !types_equal_alpha(&ty1, &ty2),
        "TyVar(Foo) should not equal Mu(α_Bar, body)"
    );
}

#[test]
fn test_tyvar_vs_mu_in_list_element() {
    // List<TyVar("TypeExpr")> should equal List<Mu("α_TypeExpr", ...)>
    // This is the actual pattern causing L2 errors
    let type_expr_as_tyvar = Type::TyVar("TypeExpr".into());
    let type_expr_as_mu = Type::mu(
        "α_TypeExpr",
        Type::sum(
            Type::Unit,
            Type::product(Type::Nat, Type::TyVar("α_TypeExpr".into())),
        ),
    );

    let list_with_tyvar = Type::mu(
        "α_List",
        Type::sum(
            Type::Unit,
            Type::product(type_expr_as_tyvar, Type::TyVar("α_List".into())),
        ),
    );
    let list_with_mu = Type::mu(
        "α_List",
        Type::sum(
            Type::Unit,
            Type::product(type_expr_as_mu, Type::TyVar("α_List".into())),
        ),
    );

    assert!(
        types_equal_alpha(&list_with_tyvar, &list_with_mu),
        "List<TyVar(T)> should equal List<Mu(α_T, ...)>"
    );
}
