//! Tests for type normalization.

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Type};

fn make_elaborator() -> Elaborator<'static> {
    // We need a static reference, so we use a leaked context
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

// ========================================================================
// Basic normalization tests
// ========================================================================

#[test]
fn test_normalize_base_types() {
    let elab = make_elaborator();

    // Base types normalize to themselves
    assert_eq!(elab.normalize_for_comparison(&Type::Nat), Type::Nat);
    assert_eq!(elab.normalize_for_comparison(&Type::Bool), Type::Bool);
    assert_eq!(elab.normalize_for_comparison(&Type::Unit), Type::Unit);
}

#[test]
fn test_structural_equality_base() {
    let elab = make_elaborator();

    assert!(elab.types_structurally_equal_normalized(&Type::Nat, &Type::Nat));
    assert!(!elab.types_structurally_equal_normalized(&Type::Nat, &Type::Bool));
}

#[test]
fn test_structural_equality_compound() {
    let elab = make_elaborator();

    let prod1 = Type::product(Type::Nat, Type::Bool);
    let prod2 = Type::product(Type::Nat, Type::Bool);
    let prod3 = Type::product(Type::Bool, Type::Nat);

    assert!(elab.types_structurally_equal_normalized(&prod1, &prod2));
    assert!(!elab.types_structurally_equal_normalized(&prod1, &prod3));
}

// ========================================================================
// Mutual recursion and cycle detection tests
// ========================================================================

#[test]
fn test_normalize_mutually_recursive_types_terminates() {
    let mut elab = make_elaborator();

    // Create mutually recursive types:
    // type A = { field: B }
    // type B = { field: A }
    let dummy_span = Span::new(0, 0);

    // Define type A with field of type B (as Type::App)
    elab.env.define_type(TypeDef {
        name: "A".to_string(),
        params: vec![],
        kind: TypeDefKind::Record(vec![("field".to_string(), Type::app("B", vec![]))]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Define type B with field of type A (as Type::App)
    elab.env.define_type(TypeDef {
        name: "B".to_string(),
        params: vec![],
        kind: TypeDefKind::Record(vec![("field".to_string(), Type::app("A", vec![]))]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // This should terminate (not hang) due to cycle detection
    // The result should contain an unexpanded Type::App to break the cycle
    let type_a = Type::app("A", vec![]);
    let normalized = elab.normalize_for_comparison(&type_a);

    // Verify it terminates - if we get here, cycle detection worked!
    // The result will be App("A", []) because:
    // - A is a single-field record { field: B }
    // - Single-field records encode to just the field type
    // - B's field type is A, and A is already being expanded (cycle)
    // - So B returns App("A", []) as the cycle breaker
    // - And A's single field becomes App("A", [])
    // The key point is that the function TERMINATES, not what the exact result is.

    // Just verify we got a result (didn't hang)
    assert!(
        !matches!(&normalized, Type::Void), // Dummy check - we care that we got here
        "Normalization completed with result: {:?}",
        normalized
    );
}

#[test]
fn test_normalize_multi_field_mutual_recursion() {
    let mut elab = make_elaborator();

    // Create mutually recursive types with multiple fields:
    // type A = { x: Nat, other: B }
    // type B = { y: Bool, other: A }
    let dummy_span = Span::new(0, 0);

    elab.env.define_type(TypeDef {
        name: "A".to_string(),
        params: vec![],
        kind: TypeDefKind::Record(vec![
            ("x".to_string(), Type::Nat),
            ("other".to_string(), Type::app("B", vec![])),
        ]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    elab.env.define_type(TypeDef {
        name: "B".to_string(),
        params: vec![],
        kind: TypeDefKind::Record(vec![
            ("y".to_string(), Type::Bool),
            ("other".to_string(), Type::app("A", vec![])),
        ]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let type_a = Type::app("A", vec![]);
    let normalized = elab.normalize_for_comparison(&type_a);

    // Records are kept as nominal types (not expanded to products)
    // This ensures consistency with how records are treated in constructors
    assert!(
        matches!(&normalized, Type::App(name, args) if name == "A" && args.is_empty()),
        "Expected records to remain nominal (App), got {:?}",
        normalized
    );
}

// ========================================================================
// Idempotence and consistency tests
// ========================================================================

/// Test that normalization is idempotent: normalize(normalize(t)) == normalize(t)
///
/// This is an important property that ensures types reach a canonical form.
/// If normalization is not idempotent, comparing two types that took different
/// paths through normalization could yield inconsistent results.
#[test]
fn test_normalization_is_idempotent() {
    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    // Define a recursive ADT: type List = Nil | Cons(Nat, List)
    elab.env.define_type(TypeDef {
        name: "List".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "Nil".to_string(),
                fields: vec![],
                index: 0,
                visibility: None,
                span: dummy_span,
            },
            Constructor {
                name: "Cons".to_string(),
                fields: vec![Type::Nat, Type::app("List", vec![])],
                index: 1,
                visibility: None,
                span: dummy_span,
            },
        ]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Normalize the type once
    let list_ty = Type::app("List", vec![]);
    let normalized_once = elab.normalize_for_comparison(&list_ty);

    // Normalize the result again
    let normalized_twice = elab.normalize_for_comparison(&normalized_once);

    // They should be identical (idempotent)
    assert_eq!(
        normalized_once, normalized_twice,
        "Normalization is not idempotent!\nOnce: {:?}\nTwice: {:?}",
        normalized_once, normalized_twice
    );
}

/// Test that types_equal produces consistent results for recursive types
/// used in different contexts (direct vs after transformation).
///
/// This is a regression test for ADR 25.1.26 section 4.3.
#[test]
fn test_types_equal_recursive_type_consistency() {
    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    // Define: type List = Nil | Cons(Nat, List)
    elab.env.define_type(TypeDef {
        name: "List".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "Nil".to_string(),
                fields: vec![],
                index: 0,
                visibility: None,
                span: dummy_span,
            },
            Constructor {
                name: "Cons".to_string(),
                fields: vec![Type::Nat, Type::app("List", vec![])],
                index: 1,
                visibility: None,
                span: dummy_span,
            },
        ]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Both representations of List should be equal
    let list_as_app = Type::app("List", vec![]);
    let list_as_tyvar = Type::TyVar("List".to_string());

    // App form vs App form
    assert!(
        elab.types_equal(&list_as_app, &list_as_app),
        "List (App) should equal itself"
    );

    // TyVar form vs TyVar form
    assert!(
        elab.types_equal(&list_as_tyvar, &list_as_tyvar),
        "List (TyVar) should equal itself"
    );

    // Cross-comparison: both should normalize to the same μ-type
    assert!(
        elab.types_equal(&list_as_app, &list_as_tyvar),
        "List (App) should equal List (TyVar) after normalization"
    );
}

// ========================================================================
// Tests for Phase 1: Type Alias Expansion Fix (ADR 1.2.26)
// ========================================================================

/// Test that Ptr<Alias> normalizes correctly when Alias is a type alias.
/// This tests the fix for Ptr types not recursively normalizing their inner types.
#[test]
fn test_ptr_alias_normalization() {
    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    // Define: type MyNat = Nat
    elab.env.define_type(TypeDef {
        name: "MyNat".to_string(),
        params: vec![],
        kind: TypeDefKind::Alias(Type::Nat),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Ptr<MyNat> should equal Ptr<Nat> after normalization
    let ptr_alias = Type::ptr(Type::TyVar("MyNat".to_string()));
    let ptr_nat = Type::ptr(Type::Nat);

    assert!(
        elab.types_equal(&ptr_alias, &ptr_nat),
        "Ptr<MyNat> should equal Ptr<Nat> after alias expansion"
    );
}

/// Test that parameterized aliases substitute type arguments correctly.
/// e.g., type Handle<T> = Ptr<T> with Handle<Nat> should equal Ptr<Nat>
#[test]
fn test_parameterized_alias_normalization() {
    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    // Define: type Handle<T> = Ptr<T>
    elab.env.define_type(TypeDef {
        name: "Handle".to_string(),
        params: vec!["T".to_string()],
        kind: TypeDefKind::Alias(Type::ptr(Type::TyVar("T".to_string()))),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Handle<Nat> should equal Ptr<Nat> after normalization
    let handle_nat = Type::app("Handle", vec![Type::Nat]);
    let ptr_nat = Type::ptr(Type::Nat);

    assert!(
        elab.types_equal(&handle_nat, &ptr_nat),
        "Handle<Nat> should equal Ptr<Nat> after parameterized alias expansion"
    );
}

/// Test nested alias expansion: Ptr<Ref<SomeAlias>> where SomeAlias = Nat
#[test]
fn test_nested_alias_expansion() {
    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    // Define: type SomeAlias = Nat
    elab.env.define_type(TypeDef {
        name: "SomeAlias".to_string(),
        params: vec![],
        kind: TypeDefKind::Alias(Type::Nat),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Ptr<Ref<SomeAlias>> should equal Ptr<Ref<Nat>> after normalization
    let with_alias = Type::ptr(Type::ref_ty(Type::TyVar("SomeAlias".to_string())));
    let without_alias = Type::ptr(Type::ref_ty(Type::Nat));

    assert!(
        elab.types_equal(&with_alias, &without_alias),
        "Ptr<Ref<SomeAlias>> should equal Ptr<Ref<Nat>> after nested alias expansion"
    );
}
