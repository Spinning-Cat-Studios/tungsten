use super::*;

#[test]
fn test_merged_binary_type_constructors() {
    // Verify Arrow, Product, Sum all work through the merged or-pattern arm
    let a = Type::arrow(Type::Nat, Type::Bool);
    let b = Type::arrow(Type::Nat, Type::Bool);
    assert!(types_equal_alpha(&a, &b));

    let p = Type::product(Type::Nat, Type::String);
    let q = Type::product(Type::Nat, Type::String);
    assert!(types_equal_alpha(&p, &q));

    let s1 = Type::sum(Type::Bool, Type::Unit);
    let s2 = Type::sum(Type::Bool, Type::Unit);
    assert!(types_equal_alpha(&s1, &s2));

    // Cross-constructor: should NOT be equal
    assert!(!types_equal_alpha(&a, &p));
    assert!(!types_equal_alpha(&p, &s1));
}

#[test]
fn test_merged_binding_forms() {
    // Mu and Forall through the merged or-pattern arm
    let mu1 = Type::mu("a", Type::arrow(Type::TyVar("a".into()), Type::Nat));
    let mu2 = Type::mu("b", Type::arrow(Type::TyVar("b".into()), Type::Nat));
    assert!(types_equal_alpha(&mu1, &mu2));

    let fa1 = Type::forall("a", Type::arrow(Type::TyVar("a".into()), Type::Nat));
    let fa2 = Type::forall("b", Type::arrow(Type::TyVar("b".into()), Type::Nat));
    assert!(types_equal_alpha(&fa1, &fa2));

    // Mu vs Forall: should NOT be equal
    assert!(!types_equal_alpha(&mu1, &fa1));
}

#[test]
fn test_with_binding_restores_env() {
    // After with_binding, the env should be restored to its prior state
    let mut env = HashMap::new();
    env.insert("a".to_owned(), "old".to_owned());

    let result = with_binding(&mut env, "a", "new", |env| {
        assert_eq!(env.get("a").unwrap(), "new");
        42
    });
    assert_eq!(result, 42);
    assert_eq!(env.get("a").unwrap(), "old"); // restored
}

#[test]
fn test_with_binding_removes_when_no_prior() {
    let mut env = HashMap::new();
    with_binding(&mut env, "fresh", "val", |env| {
        assert_eq!(env.get("fresh").unwrap(), "val");
    });
    assert!(!env.contains_key("fresh")); // removed
}

#[test]
fn test_all_types_equal_empty() {
    let mut env = HashMap::new();
    assert!(all_types_equal(&[], &[], &mut env));
}

#[test]
fn test_all_types_equal_length_mismatch() {
    let mut env = HashMap::new();
    assert!(!all_types_equal(&[Type::Nat], &[], &mut env));
}

#[test]
fn test_all_types_equal_elements() {
    let mut env = HashMap::new();
    assert!(all_types_equal(
        &[Type::Nat, Type::Bool],
        &[Type::Nat, Type::Bool],
        &mut env,
    ));
    assert!(!all_types_equal(
        &[Type::Nat, Type::Bool],
        &[Type::Nat, Type::String],
        &mut env,
    ));
}

#[test]
fn test_tyvar_app_equal_zero_arity() {
    let env = HashMap::new();
    assert!(tyvar_app_equal("Foo", "Foo", &[], &env));
    assert!(!tyvar_app_equal("Foo", "Bar", &[], &env));
}

#[test]
fn test_tyvar_app_equal_nonzero_arity_returns_false() {
    let env = HashMap::new();
    assert!(!tyvar_app_equal("Foo", "Foo", &[Type::Nat], &env));
}

#[test]
fn test_tyvar_app_equal_with_at_prefix() {
    let env = HashMap::new();
    assert!(tyvar_app_equal("@Foo", "Foo", &[], &env));
}

#[test]
fn test_tyvar_app_equal_with_env_binding() {
    let mut env = HashMap::new();
    env.insert("X".to_string(), "Y".to_string());
    assert!(tyvar_app_equal("X", "Y", &[], &env));
    assert!(!tyvar_app_equal("X", "Z", &[], &env));
}
