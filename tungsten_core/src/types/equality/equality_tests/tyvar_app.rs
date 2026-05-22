use super::*;

#[test]
fn test_tyvar_equals_zero_arity_app() {
    // TyVar("X") should equal App("X", [])
    let ty1 = Type::TyVar("CodegenType".into());
    let ty2 = Type::app("CodegenType", vec![]);
    assert!(
        types_equal_alpha(&ty1, &ty2),
        "TyVar(X) should equal App(X, [])"
    );
}

#[test]
fn test_zero_arity_app_equals_tyvar() {
    // Symmetric case: App("X", []) should equal TyVar("X")
    let ty1 = Type::app("TypeExpr", vec![]);
    let ty2 = Type::TyVar("TypeExpr".into());
    assert!(
        types_equal_alpha(&ty1, &ty2),
        "App(X, []) should equal TyVar(X)"
    );
}

#[test]
fn test_tyvar_vs_app_in_list_arg() {
    // List<TyVar("T")> should equal List<App("T", [])>
    let ty1 = Type::app("List", vec![Type::TyVar("CodegenType".into())]);
    let ty2 = Type::app("List", vec![Type::app("CodegenType", vec![])]);
    assert!(
        types_equal_alpha(&ty1, &ty2),
        "List<TyVar(T)> should equal List<App(T, [])>"
    );
}

#[test]
fn test_tyvar_vs_app_in_mu_body() {
    // Inside a Mu body, TyVar("X") should equal App("X", [])
    let ty1 = Type::mu(
        "α_List",
        Type::sum(
            Type::Unit,
            Type::product(Type::TyVar("TypeExpr".into()), Type::TyVar("α_List".into())),
        ),
    );
    let ty2 = Type::mu(
        "α_List",
        Type::sum(
            Type::Unit,
            Type::product(Type::app("TypeExpr", vec![]), Type::TyVar("α_List".into())),
        ),
    );
    assert!(
        types_equal_alpha(&ty1, &ty2),
        "TyVar vs App(_, []) should be equal inside Mu bodies"
    );
}

#[test]
fn test_tyvar_vs_app_in_product() {
    // In a product type: (TyVar("A") × TyVar("B")) should equal (App("A", []) × App("B", []))
    let ty1 = Type::product(Type::TyVar("A".into()), Type::TyVar("B".into()));
    let ty2 = Type::product(Type::app("A", vec![]), Type::app("B", vec![]));
    assert!(
        types_equal_alpha(&ty1, &ty2),
        "Products with TyVar vs App(_, []) should be equal"
    );
}
