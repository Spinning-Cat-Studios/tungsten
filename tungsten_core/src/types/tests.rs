use super::*;

#[test]
fn test_type_display() {
    assert_eq!(Type::Bool.to_string(), "Bool");
    assert_eq!(
        Type::arrow(Type::Nat, Type::Bool).to_string(),
        "(Nat → Bool)"
    );
    assert_eq!(
        Type::product(Type::Bool, Type::Nat).to_string(),
        "(Bool × Nat)"
    );
    assert_eq!(
        Type::sum(Type::Unit, Type::Void).to_string(),
        "(Unit + Void)"
    );
    assert_eq!(
        Type::forall("α", Type::TyVar("α".into())).to_string(),
        "∀α. α"
    );
}

#[test]
fn test_type_substitution() {
    let ty = Type::TyVar("α".into());
    let result = ty.substitute("α", &Type::Nat);
    assert_eq!(result, Type::Nat);

    let arrow = Type::arrow(Type::TyVar("α".into()), Type::TyVar("α".into()));
    let result = arrow.substitute("α", &Type::Bool);
    assert_eq!(result, Type::arrow(Type::Bool, Type::Bool));
}

#[test]
fn test_forall_shadowing() {
    // ∀α. α should not substitute inner α
    let ty = Type::forall("α", Type::TyVar("α".into()));
    let result = ty.substitute("α", &Type::Nat);
    assert_eq!(result, Type::forall("α", Type::TyVar("α".into())));
}

#[test]
fn test_free_type_vars() {
    let ty = Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into()));
    let free = ty.free_type_vars();
    assert!(free.contains("α"));
    assert!(free.contains("β"));
    assert_eq!(free.len(), 2);

    // Forall binds α
    let ty = Type::forall(
        "α",
        Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into())),
    );
    let free = ty.free_type_vars();
    assert!(!free.contains("α"));
    assert!(free.contains("β"));
    assert_eq!(free.len(), 1);
}

// ======================================================================
// Type::substitute — base type passthrough and binder shadowing
// ======================================================================

#[test]
fn test_type_substitute_base_types_unchanged() {
    for base in &[
        Type::Bool,
        Type::Nat,
        Type::Unit,
        Type::Void,
        Type::Prop,
        Type::String,
        Type::Error,
    ] {
        assert_eq!(base.substitute("α", &Type::Nat), base.clone());
    }
}

#[test]
fn test_type_substitute_forall_shadows() {
    // ∀α. α → α — substituting α should not penetrate
    let ty = Type::forall(
        "α",
        Type::arrow(Type::TyVar("α".into()), Type::TyVar("α".into())),
    );
    let result = ty.substitute("α", &Type::Bool);
    assert_eq!(result, ty);
}

#[test]
fn test_type_substitute_forall_no_shadow() {
    // ∀α. α → β — substituting β should work
    let ty = Type::forall(
        "α",
        Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into())),
    );
    let result = ty.substitute("β", &Type::Nat);
    assert_eq!(
        result,
        Type::forall("α", Type::arrow(Type::TyVar("α".into()), Type::Nat))
    );
}

#[test]
fn test_type_substitute_mu_shadows() {
    // μα. Unit + α — substituting α should not penetrate
    let ty = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::Unit, Type::TyVar("α".into()))),
    );
    let result = ty.substitute("α", &Type::Bool);
    assert_eq!(result, ty);
}

#[test]
fn test_type_substitute_mu_no_shadow() {
    // μα. β + α — substituting β should work
    let ty = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::TyVar("β".into()), Type::TyVar("α".into()))),
    );
    let result = ty.substitute("β", &Type::Nat);
    assert_eq!(
        result,
        Type::Mu(
            "α".into(),
            Box::new(Type::sum(Type::Nat, Type::TyVar("α".into())))
        )
    );
}

// ======================================================================
// Type::reconstruct_* helpers
// ======================================================================

#[test]
fn test_reconstruct_binary_arrow() {
    let template = Type::arrow(Type::Unit, Type::Unit);
    let result = Type::reconstruct_binary(&template, Type::Nat, Type::Bool);
    assert_eq!(result, Type::arrow(Type::Nat, Type::Bool));
}

#[test]
fn test_reconstruct_binary_product() {
    let template = Type::product(Type::Unit, Type::Unit);
    let result = Type::reconstruct_binary(&template, Type::Nat, Type::Bool);
    assert_eq!(result, Type::product(Type::Nat, Type::Bool));
}

#[test]
fn test_reconstruct_binary_sum() {
    let template = Type::sum(Type::Unit, Type::Unit);
    let result = Type::reconstruct_binary(&template, Type::Nat, Type::Bool);
    assert_eq!(result, Type::sum(Type::Nat, Type::Bool));
}

#[test]
fn test_reconstruct_binding_forall() {
    let template = Type::forall("x", Type::Unit);
    let result = Type::reconstruct_binding(&template, "α", Type::Nat);
    assert_eq!(result, Type::forall("α", Type::Nat));
}

#[test]
fn test_reconstruct_binding_mu() {
    let template = Type::mu("x", Type::Unit);
    let result = Type::reconstruct_binding(&template, "α", Type::Nat);
    assert_eq!(result, Type::mu("α", Type::Nat));
}

#[test]
fn test_reconstruct_wrapper_ptr() {
    let template = Type::ptr(Type::Unit);
    let result = Type::reconstruct_wrapper(&template, Type::Nat);
    assert_eq!(result, Type::ptr(Type::Nat));
}

#[test]
fn test_reconstruct_wrapper_ref() {
    let template = Type::ref_ty(Type::Unit);
    let result = Type::reconstruct_wrapper(&template, Type::Nat);
    assert_eq!(result, Type::ref_ty(Type::Nat));
}

#[test]
#[should_panic(expected = "reconstruct_binary called on non-binary type")]
fn test_reconstruct_binary_panics_on_non_binary() {
    Type::reconstruct_binary(&Type::Nat, Type::Unit, Type::Unit);
}

#[test]
#[should_panic(expected = "reconstruct_binding called on non-binding type")]
fn test_reconstruct_binding_panics_on_non_binding() {
    Type::reconstruct_binding(&Type::Nat, "x", Type::Unit);
}

#[test]
#[should_panic(expected = "reconstruct_wrapper called on non-wrapper type")]
fn test_reconstruct_wrapper_panics_on_non_wrapper() {
    Type::reconstruct_wrapper(&Type::Nat, Type::Unit);
}

// ======================================================================
// Type::node_count — count nodes in type tree
// ======================================================================

#[test]
fn test_node_count_leaf_types() {
    assert_eq!(Type::Nat.node_count(), 1);
    assert_eq!(Type::Bool.node_count(), 1);
    assert_eq!(Type::Unit.node_count(), 1);
    assert_eq!(Type::TyVar("α".into()).node_count(), 1);
    assert_eq!(Type::Error.node_count(), 1);
}

#[test]
fn test_node_count_binary() {
    // Arrow(Nat, Bool) = 3 nodes
    assert_eq!(Type::arrow(Type::Nat, Type::Bool).node_count(), 3);
    // Product(Sum(Nat, Bool), Unit) = 5 nodes
    assert_eq!(
        Type::product(Type::sum(Type::Nat, Type::Bool), Type::Unit).node_count(),
        5
    );
}

#[test]
fn test_node_count_mu() {
    // Mu(α, Sum(Unit, TyVar(α))) = 1 + 1 + 1 + 1 = 4
    let mu = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::Unit, Type::TyVar("α".into()))),
    );
    assert_eq!(mu.node_count(), 4);
}

#[test]
fn test_node_count_nested_mu() {
    // Mu(α_A, Mu(α_B, Sum(TyVar(α_A), TyVar(α_B)))) = 5
    let ty = Type::Mu(
        "α_A".into(),
        Box::new(Type::Mu(
            "α_B".into(),
            Box::new(Type::sum(
                Type::TyVar("α_A".into()),
                Type::TyVar("α_B".into()),
            )),
        )),
    );
    assert_eq!(ty.node_count(), 5);
}

// ======================================================================
// Type::depth — max depth of type tree
// ======================================================================

#[test]
fn test_depth_leaf_types() {
    assert_eq!(Type::Nat.depth(), 1);
    assert_eq!(Type::Bool.depth(), 1);
    assert_eq!(Type::TyVar("α".into()).depth(), 1);
}

#[test]
fn test_depth_binary() {
    // Arrow(Nat, Bool) → depth 2
    assert_eq!(Type::arrow(Type::Nat, Type::Bool).depth(), 2);
}

#[test]
fn test_depth_nested() {
    // Arrow(Nat, Arrow(Bool, Unit)) → depth 3
    assert_eq!(
        Type::arrow(Type::Nat, Type::arrow(Type::Bool, Type::Unit)).depth(),
        3
    );
}

#[test]
fn test_depth_mu() {
    // Mu(α, Sum(Unit, TyVar(α))) → depth 3
    let mu = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::Unit, Type::TyVar("α".into()))),
    );
    assert_eq!(mu.depth(), 3);
}

#[test]
fn test_depth_asymmetric_tree() {
    // Product(Nat, Arrow(Bool, Arrow(Unit, Void)))
    // Left: depth 1, Right: depth 3 → total 4
    let ty = Type::product(
        Type::Nat,
        Type::arrow(Type::Bool, Type::arrow(Type::Unit, Type::Void)),
    );
    assert_eq!(ty.depth(), 4);
}
