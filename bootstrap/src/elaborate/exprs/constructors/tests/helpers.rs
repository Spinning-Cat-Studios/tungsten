//! Tests for refactored helper functions and cross-module constructor injection.
//!
//! Covers:
//! - build_product_value
//! - wrap_in_fold_if_recursive
//! - validate_ctor_arity
//! - get_constructor_context
//! - types_pattern_match
//! - infer_type_args_from_constructor
//! - Cross-module constructor injection (ADR 30.1.26 Category A Fix)

use crate::ast::Visibility;
use crate::elaborate::env::{self, Constructor, TypeDef, TypeDefKind};
use crate::elaborate::exprs::constructors::context::ConstructorContext;
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Term, Type};

/// Create an Elaborator for testing.
fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

// ========================================================================
// Tests for refactored helper functions
// ========================================================================

/// Test build_product_value with empty args returns Unit.
#[test]
fn test_build_product_value_empty() {
    let elab = make_elaborator();
    let result = elab.build_product_value(vec![]);
    assert_eq!(result, Term::Unit);
}

/// Test build_product_value with single arg returns that arg.
#[test]
fn test_build_product_value_single() {
    let elab = make_elaborator();
    let term = Term::var("x");
    let result = elab.build_product_value(vec![term.clone()]);
    assert_eq!(result, term);
}

/// Test build_product_value with multiple args builds right-nested pairs.
#[test]
fn test_build_product_value_multiple() {
    let elab = make_elaborator();
    let a = Term::var("a");
    let b = Term::var("b");
    let c = Term::var("c");
    let result = elab.build_product_value(vec![a.clone(), b.clone(), c.clone()]);
    // Should be (a, (b, c))
    let expected = Term::pair(a, Term::pair(b, c));
    assert_eq!(result, expected);
}

/// Test wrap_in_fold_if_recursive wraps when recursive.
#[test]
fn test_wrap_in_fold_if_recursive_true() {
    let elab = make_elaborator();
    let term = Term::Unit;
    let ty = Type::Nat;
    let result = elab.wrap_in_fold_if_recursive(term.clone(), ty.clone(), true);
    assert_eq!(result, Term::fold(ty, term));
}

/// Test wrap_in_fold_if_recursive doesn't wrap when not recursive.
#[test]
fn test_wrap_in_fold_if_recursive_false() {
    let elab = make_elaborator();
    let term = Term::Unit;
    let ty = Type::Nat;
    let result = elab.wrap_in_fold_if_recursive(term.clone(), ty, false);
    assert_eq!(result, term);
}

/// Test validate_ctor_arity passes on match.
#[test]
fn test_validate_ctor_arity_match() {
    let elab = make_elaborator();
    let span = Span::new(0, 0);
    let result = elab.validate_ctor_arity("Test", 2, 2, span);
    assert!(result.is_ok());
}

/// Test validate_ctor_arity fails on mismatch.
#[test]
fn test_validate_ctor_arity_mismatch() {
    let elab = make_elaborator();
    let span = Span::new(0, 0);
    let result = elab.validate_ctor_arity("Test", 2, 3, span);
    assert!(result.is_err());
}

/// Test get_constructor_context returns error for missing type.
#[test]
fn test_get_constructor_context_missing_type() {
    let elab = make_elaborator();
    let span = Span::new(0, 0);
    let info = env::ConstructorInfo::test_stub("NonExistent", 0, 0);
    let result = elab.get_constructor_context(&info, span);
    assert!(result.is_err());
}

/// Test get_constructor_context returns correct info for valid ADT.
#[test]
fn test_get_constructor_context_valid() {
    let mut elab = make_elaborator();
    let span = Span::new(0, 0);

    // Register Option<T> ADT
    elab.env.define_type(TypeDef {
        name: "Option".to_string(),
        params: vec!["T".to_string()],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "None".to_string(),
                fields: vec![],
                index: 0,
                span,
                visibility: None,
            },
            Constructor {
                name: "Some".to_string(),
                fields: vec![Type::TyVar("T".to_string())],
                index: 1,
                span,
                visibility: None,
            },
        ]),
        visibility: Visibility::Public,
        span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let info = env::ConstructorInfo::test_stub("Option", 0, 0);
    let result = elab.get_constructor_context(&info, span);
    assert!(result.is_ok());
    let ctx = result.unwrap();
    assert_eq!(ctx.constructors.len(), 2);
    assert_eq!(ctx.type_params, vec!["T".to_string()]);
    assert!(!ctx.is_recursive); // Option is not recursive
}

/// Test types_pattern_match with type variable matches anything.
#[test]
fn test_types_pattern_match_tyvar_matches_any() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];

    // TyVar("T") should match Nat
    assert!(elab.types_pattern_match(&Type::TyVar("T".to_string()), &Type::Nat, &type_params));

    // TyVar("T") should match String
    assert!(elab.types_pattern_match(&Type::TyVar("T".to_string()), &Type::String, &type_params));

    // TyVar("T") should match Product
    assert!(elab.types_pattern_match(
        &Type::TyVar("T".to_string()),
        &Type::product(Type::Nat, Type::Bool),
        &type_params
    ));
}

/// Test types_pattern_match with non-param type var requires equality.
#[test]
fn test_types_pattern_match_tyvar_not_param() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()]; // U is not a type param

    // TyVar("U") should only match TyVar("U")
    assert!(elab.types_pattern_match(
        &Type::TyVar("U".to_string()),
        &Type::TyVar("U".to_string()),
        &type_params
    ));

    // TyVar("U") should NOT match TyVar("V")
    assert!(!elab.types_pattern_match(
        &Type::TyVar("U".to_string()),
        &Type::TyVar("V".to_string()),
        &type_params
    ));
}

/// Test types_pattern_match with compound types.
#[test]
fn test_types_pattern_match_compound() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];

    // Sum(Unit, T) should match Sum(Unit, Nat)
    let pattern = Type::sum(Type::Unit, Type::TyVar("T".to_string()));
    let concrete = Type::sum(Type::Unit, Type::Nat);
    assert!(elab.types_pattern_match(&pattern, &concrete, &type_params));

    // Sum(Unit, T) should NOT match Sum(Bool, Nat) - first component differs
    let concrete2 = Type::sum(Type::Bool, Type::Nat);
    assert!(!elab.types_pattern_match(&pattern, &concrete2, &type_params));
}

/// Test infer_type_args_from_constructor.
#[test]
fn test_infer_type_args_from_constructor() {
    let elab = make_elaborator();
    let span = Span::new(0, 0);

    // Type params: [T], field types: [T], arg types: [Nat]
    // Should infer T = Nat
    let type_params = vec!["T".to_string()];
    let field_types = vec![Type::TyVar("T".to_string())];
    let arg_types = vec![Type::Nat];

    let result = elab
        .infer_type_args_from_constructor(&type_params, &field_types, &arg_types, span)
        .unwrap();

    assert_eq!(result, vec![Type::Nat]);
}

/// Test infer_type_args_from_constructor with multiple params.
#[test]
fn test_infer_type_args_from_constructor_multiple() {
    let elab = make_elaborator();
    let span = Span::new(0, 0);

    // Type params: [A, B], field types: [A, B], arg types: [Nat, String]
    // Should infer A = Nat, B = String
    let type_params = vec!["A".to_string(), "B".to_string()];
    let field_types = vec![Type::TyVar("A".to_string()), Type::TyVar("B".to_string())];
    let arg_types = vec![Type::Nat, Type::String];

    let result = elab
        .infer_type_args_from_constructor(&type_params, &field_types, &arg_types, span)
        .unwrap();

    assert_eq!(result, vec![Type::Nat, Type::String]);
}

// ========================================================================
// Tests for cross-module constructor injection (ADR 30.1.26 Category A Fix)
// ========================================================================
//
// When constructors are used with cross-module ADT types, the type may be
// represented as Type::App("TypeName", []) instead of its structural μ-encoding.
// The injection builder must normalize such types before traversing the sum.

/// Helper to register a simple ADT type for testing.
fn register_test_adt(elab: &mut Elaborator, name: &str, num_ctors: usize) {
    let constructors: Vec<Constructor> = (0..num_ctors)
        .map(|i| Constructor {
            name: format!("Ctor{}", i),
            fields: vec![], // Nullary constructors
            index: i,       // Constructor index in the ADT
            span: Span::new(0, 0),
            visibility: None,
        })
        .collect();

    let type_def = TypeDef {
        name: name.to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(constructors),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    };

    elab.env.define_type(type_def);
}

/// Test injection with a proper μ-encoded ADT type (baseline - should work).
/// Note: For n >= 3, we use build_constructor_term which emits Term::adt_construct.
/// build_constructor_injection is only for n = 2 (binary sum injection).
#[test]
fn test_injection_with_mu_encoded_type() {
    let mut elab = make_elaborator();
    register_test_adt(&mut elab, "TestADT", 3);

    // Encode the ADT properly - for n=3 this is Type::Adt (ADR 2.2.26)
    let adt_type = elab.encode_adt_type("TestADT", &[]).unwrap();

    // Build constructor term for constructor 0 (first)
    // For n >= 3, this uses Term::adt_construct, not inl/inr chains
    let result = elab.build_constructor_term(Term::Unit, 0, 3, &adt_type, false);
    assert!(
        result.is_ok(),
        "constructor term for first constructor should succeed"
    );

    // Build constructor term for constructor 1 (middle)
    let result = elab.build_constructor_term(Term::Unit, 1, 3, &adt_type, false);
    assert!(
        result.is_ok(),
        "constructor term for middle constructor should succeed"
    );

    // Build constructor term for constructor 2 (last)
    let result = elab.build_constructor_term(Term::Unit, 2, 3, &adt_type, false);
    assert!(
        result.is_ok(),
        "constructor term for last constructor should succeed"
    );
}

/// Test injection with Type::App cross-module reference (the Category A fix).
/// Before the fix, this would fail with "expected sum type in constructor injection".
/// Note: For n >= 3, we use build_constructor_term which handles Type::App normalization.
#[test]
fn test_injection_with_cross_module_type_app() {
    let mut elab = make_elaborator();
    register_test_adt(&mut elab, "CrossModuleADT", 3);

    // Simulate a cross-module type reference as Type::App("CrossModuleADT", [])
    // This is how types appear when referenced from another module before
    // full resolution.
    let cross_module_type = Type::App("CrossModuleADT".to_string(), vec![]);

    // Build constructor term for constructor 0
    // For n >= 3, this uses Term::adt_construct, not inl/inr chains
    let result = elab.build_constructor_term(Term::Unit, 0, 3, &cross_module_type, false);
    assert!(
        result.is_ok(),
        "constructor term with Type::App cross-module reference should succeed"
    );

    // Build constructor term for constructor 1
    let result = elab.build_constructor_term(Term::Unit, 1, 3, &cross_module_type, false);
    assert!(
        result.is_ok(),
        "constructor term for middle constructor with cross-module type should succeed"
    );

    // Build constructor term for constructor 2
    let result = elab.build_constructor_term(Term::Unit, 2, 3, &cross_module_type, false);
    assert!(
        result.is_ok(),
        "constructor term for last constructor with cross-module type should succeed"
    );
}

/// Test injection with Type::TyVar (another form of type reference).
#[test]
fn test_injection_with_tyvar_reference() {
    let mut elab = make_elaborator();
    register_test_adt(&mut elab, "TyVarADT", 2);

    // Type::TyVar is used for local type references
    let tyvar_type = Type::TyVar("TyVarADT".to_string());

    // Build injection for constructor 0
    let result = elab.build_constructor_injection(Term::Unit, 0, 2, &tyvar_type);
    assert!(
        result.is_ok(),
        "injection with TyVar reference should succeed after normalization"
    );

    // Build injection for constructor 1
    let result = elab.build_constructor_injection(Term::Unit, 1, 2, &tyvar_type);
    assert!(
        result.is_ok(),
        "injection for last constructor with TyVar should succeed"
    );
}

/// Test that unknown types still produce errors (not silently succeed).
/// Note: For index=0 with num_ctors>1, the descent loop runs 0 times,
/// so the error isn't triggered. We test with index=1 to ensure descent.
#[test]
fn test_injection_with_unknown_type_fails() {
    let elab = make_elaborator();
    // Don't register any type - use an unknown type reference
    let unknown_type = Type::App("UnknownType".to_string(), vec![]);

    // For index=1, we need to descend into the sum type structure.
    // Since UnknownType can't be normalized to a sum, this should fail.
    let result = elab.build_constructor_injection(Term::Unit, 1, 2, &unknown_type);
    assert!(
        result.is_err(),
        "injection with unknown type should fail when descent is needed"
    );
}
