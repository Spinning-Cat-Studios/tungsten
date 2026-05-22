use super::*;
use crate::ast::Visibility;
use crate::span::Span;

#[test]
fn test_env_new() {
    let env = Env::new();
    assert!(env.types.is_empty());
    assert!(env.values.is_empty());
    assert!(env.constructors.is_empty());
}

#[test]
fn test_define_value() {
    let mut env = Env::new();
    env.define_value(ValueDef {
        name: "foo".to_string(),
        ty: Type::Nat,
        visibility: Visibility::Private,
        span: Span::new(0, 3),
    });

    assert!(env.has_value("foo"));
    assert!(!env.has_value("bar"));

    let def = env.lookup_value("foo").unwrap();
    assert_eq!(def.name, "foo");
    assert_eq!(def.ty, Type::Nat);
}

#[test]
fn test_local_scopes() {
    let mut env = Env::new();

    // Enter scope and bind x
    env.push_scope();
    env.bind_local("x".to_string(), Type::Nat, 0);

    // Can resolve x
    assert!(env.lookup_local("x").is_some());

    // Enter nested scope and bind y
    env.push_scope();
    env.bind_local("y".to_string(), Type::Bool, 1);

    // Can resolve both
    assert!(env.lookup_local("x").is_some());
    assert!(env.lookup_local("y").is_some());

    // Exit nested scope
    env.pop_scope();

    // y is gone, x remains
    assert!(env.lookup_local("x").is_some());
    assert!(env.lookup_local("y").is_none());

    // Exit outer scope
    env.pop_scope();
    assert!(env.lookup_local("x").is_none());
}

#[test]
fn test_type_vars() {
    let mut env = Env::new();

    assert!(!env.has_type_var("T"));

    env.push_type_var("T".to_string());
    assert!(env.has_type_var("T"));

    env.push_type_var("U".to_string());
    assert!(env.has_type_var("T"));
    assert!(env.has_type_var("U"));

    env.pop_type_var();
    assert!(env.has_type_var("T"));
    assert!(!env.has_type_var("U"));
}

#[test]
fn test_resolve_value_local() {
    let mut env = Env::new();
    env.push_scope();
    env.bind_local("x".to_string(), Type::Nat, 0);

    let resolved = env.resolve_value("x", 1);
    assert!(matches!(resolved, Some(ResolvedValue::Local(0, Type::Nat))));
}

#[test]
fn test_resolve_value_global() {
    let mut env = Env::new();
    env.define_value(ValueDef {
        name: "foo".to_string(),
        ty: Type::Nat,
        visibility: Visibility::Private,
        span: Span::new(0, 3),
    });

    let resolved = env.resolve_value("foo", 0);
    assert!(matches!(
        resolved,
        Some(ResolvedValue::Global(_, Type::Nat))
    ));
}

// ── Import alias tests ──────────────────────────────────────────────

#[test]
fn test_is_name_aliased_away_positive() {
    let mut env = Env::new();
    let module = ModulePath::from_name("test");
    env.register_module(module.clone());

    // Import "add" under alias "plus"
    env.add_value_import(
        &module,
        ImportRequest {
            local_name: "plus".to_string(),
            source_module: ModulePath::from_name("math"),
            original_name: "add".to_string(),
            span: Span::new(0, 10),
            is_reexport: false,
            reexport_visibility: None,
        },
    );

    // "add" should be aliased away
    assert!(env.is_name_aliased_away(&module, "add"));
    // "plus" should NOT be aliased away (it's the alias itself)
    assert!(!env.is_name_aliased_away(&module, "plus"));
}

#[test]
fn test_is_name_aliased_away_normal_import() {
    let mut env = Env::new();
    let module = ModulePath::from_name("test");
    env.register_module(module.clone());

    // Normal import: local_name == original_name
    env.add_value_import(
        &module,
        ImportRequest {
            local_name: "add".to_string(),
            source_module: ModulePath::from_name("math"),
            original_name: "add".to_string(),
            span: Span::new(0, 10),
            is_reexport: false,
            reexport_visibility: None,
        },
    );

    // "add" should NOT be aliased away (it was imported as itself)
    assert!(!env.is_name_aliased_away(&module, "add"));
}

#[test]
fn test_is_name_aliased_away_type_import() {
    let mut env = Env::new();
    let module = ModulePath::from_name("test");
    env.register_module(module.clone());

    // Import type "Point" under alias "Pt"
    env.add_type_import(
        &module,
        ImportRequest {
            local_name: "Pt".to_string(),
            source_module: ModulePath::from_name("math"),
            original_name: "Point".to_string(),
            span: Span::new(0, 10),
            is_reexport: false,
            reexport_visibility: None,
        },
    );

    // "Point" should be aliased away
    assert!(env.is_name_aliased_away(&module, "Point"));
    // "Pt" should not be aliased away
    assert!(!env.is_name_aliased_away(&module, "Pt"));
    // unrelated name should not be aliased away
    assert!(!env.is_name_aliased_away(&module, "Foo"));
}
