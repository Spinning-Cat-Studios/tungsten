//! Tests for constructor elaboration.

use super::context::ConstructorContext;
use crate::ast::Visibility;
use crate::elaborate::env::{self, Constructor, TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Term, Type};

/// Create an Elaborator for testing.
fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

// ========================================================================
// Tests for `contains_free_type_vars` (ADR 30.1.26.2 fix)
// ========================================================================
//
// The fix adds a guard to `try_match_adt_type` that rejects types
// containing free type variables (like TyVar("T")), because these
// indicate a generic context, not a fully-instantiated concrete ADT.

/// A base type (Nat) contains no free type variables.
#[test]
fn test_contains_free_type_vars_base_type() {
    let elab = make_elaborator();
    assert!(!elab.contains_free_type_vars(&Type::Nat));
    assert!(!elab.contains_free_type_vars(&Type::Bool));
    assert!(!elab.contains_free_type_vars(&Type::String));
    assert!(!elab.contains_free_type_vars(&Type::Unit));
}

/// A free type variable like TyVar("T") should be detected.
#[test]
fn test_contains_free_type_vars_tyvar() {
    let elab = make_elaborator();
    let ty = Type::TyVar("T".to_string());
    assert!(elab.contains_free_type_vars(&ty));
}

/// A μ-bound variable like α_List is NOT a free type variable.
/// These are recursion markers, not generic type parameters.
#[test]
fn test_contains_free_type_vars_mu_bound() {
    let elab = make_elaborator();
    let ty = Type::TyVar("α_List".to_string());
    assert!(!elab.contains_free_type_vars(&ty));
}

/// Sum type containing a free type variable.
#[test]
fn test_contains_free_type_vars_sum_with_tyvar() {
    let elab = make_elaborator();
    // Sum(Unit, TyVar("T")) - like part of Option<T> encoding
    let ty = Type::sum(Type::Unit, Type::TyVar("T".to_string()));
    assert!(elab.contains_free_type_vars(&ty));
}

/// Sum type with no free type variables.
#[test]
fn test_contains_free_type_vars_sum_concrete() {
    let elab = make_elaborator();
    // Sum(Unit, Nat) - like Option<Nat> body
    let ty = Type::sum(Type::Unit, Type::Nat);
    assert!(!elab.contains_free_type_vars(&ty));
}

/// Product containing a free type variable.
#[test]
fn test_contains_free_type_vars_product_with_tyvar() {
    let elab = make_elaborator();
    // Product(TyVar("T"), TyVar("α_List")) - like Cons(T, List<T>) encoding
    let ty = Type::product(
        Type::TyVar("T".to_string()),
        Type::TyVar("α_List".to_string()),
    );
    // Contains T which is free, but α_List is a μ-bound var (not free)
    assert!(elab.contains_free_type_vars(&ty));
}

/// Product with only μ-bound variable - no free type vars.
#[test]
fn test_contains_free_type_vars_product_mu_bound_only() {
    let elab = make_elaborator();
    // Product(Nat, TyVar("α_List"))
    let ty = Type::product(Type::Nat, Type::TyVar("α_List".to_string()));
    assert!(!elab.contains_free_type_vars(&ty));
}

/// μ-type with free type var in body.
/// This is the key bug case: Mu("α_List", Sum(Unit, Product(TyVar("T"), α_List)))
#[test]
fn test_contains_free_type_vars_mu_with_free_tyvar() {
    let elab = make_elaborator();
    // Mu("α_List", Sum(Unit, Product(TyVar("T"), TyVar("α_List"))))
    // This represents List<T> before instantiation - T is still free
    let body = Type::sum(
        Type::Unit,
        Type::product(
            Type::TyVar("T".to_string()),
            Type::TyVar("α_List".to_string()),
        ),
    );
    let ty = Type::mu("α_List", body);
    assert!(elab.contains_free_type_vars(&ty));
}

/// μ-type with no free type vars (fully instantiated).
/// Mu("α_List", Sum(Unit, Product(String, α_List))) - List<String>
#[test]
fn test_contains_free_type_vars_mu_instantiated() {
    let elab = make_elaborator();
    // Mu("α_List", Sum(Unit, Product(String, TyVar("α_List"))))
    let body = Type::sum(
        Type::Unit,
        Type::product(Type::String, Type::TyVar("α_List".to_string())),
    );
    let ty = Type::mu("α_List", body);
    assert!(!elab.contains_free_type_vars(&ty));
}

/// Arrow type with free type variable.
#[test]
fn test_contains_free_type_vars_arrow() {
    let elab = make_elaborator();
    // T -> Nat
    let ty = Type::arrow(Type::TyVar("T".to_string()), Type::Nat);
    assert!(elab.contains_free_type_vars(&ty));
}

/// Forall binds its variable, so ∀T. T -> T has no free vars.
#[test]
fn test_contains_free_type_vars_forall_bound() {
    let elab = make_elaborator();
    // ∀T. T -> T
    let body = Type::arrow(Type::TyVar("T".to_string()), Type::TyVar("T".to_string()));
    let ty = Type::forall("T", body);
    assert!(!elab.contains_free_type_vars(&ty));
}

/// Forall with a different free variable.
#[test]
fn test_contains_free_type_vars_forall_with_free() {
    let elab = make_elaborator();
    // ∀T. T -> U (U is free)
    let body = Type::arrow(Type::TyVar("T".to_string()), Type::TyVar("U".to_string()));
    let ty = Type::forall("T", body);
    assert!(elab.contains_free_type_vars(&ty));
}

/// Type::App with free type variable in args.
#[test]
fn test_contains_free_type_vars_app() {
    let elab = make_elaborator();
    // List<T> as Type::App("List", [TyVar("T")])
    let ty = Type::app("List", vec![Type::TyVar("T".to_string())]);
    assert!(elab.contains_free_type_vars(&ty));
}

/// Type::App with concrete args.
#[test]
fn test_contains_free_type_vars_app_concrete() {
    let elab = make_elaborator();
    // List<String> as Type::App("List", [String])
    let ty = Type::app("List", vec![Type::String]);
    assert!(!elab.contains_free_type_vars(&ty));
}

// ========================================================================
// Integration tests: try_match_adt_type with free type vars
// ========================================================================
//
// These tests verify that `try_match_adt_type` correctly rejects types
// that contain free type variables, preventing the mis-detection that
// caused the original bug.

/// A sum type with free type var should NOT match any ADT.
#[test]
fn test_try_match_adt_type_rejects_free_tyvar() {
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
            },
            Constructor {
                name: "Some".to_string(),
                fields: vec![Type::TyVar("T".to_string())],
                index: 1,
                span,
            },
        ]),
        visibility: Visibility::Public,
        span,
        defining_module: None,
        encoded_type: None,
    });

    // A sum with free TyVar("T") should NOT match Option
    // This is the type *inside* a generic List<T>'s μ-body
    let sum_with_tyvar = Type::sum(Type::Unit, Type::TyVar("T".to_string()));

    // The fix: this should return None because the type has free type vars
    assert_eq!(elab.try_match_adt_type(&sum_with_tyvar), None);
}

/// A sum type with concrete types CAN match an ADT.
#[test]
fn test_try_match_adt_type_accepts_concrete() {
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
            },
            Constructor {
                name: "Some".to_string(),
                fields: vec![Type::TyVar("T".to_string())],
                index: 1,
                span,
            },
        ]),
        visibility: Visibility::Public,
        span,
        defining_module: None,
        encoded_type: None,
    });

    // Sum(Unit, Nat) - this is Option<Nat> encoded
    let sum_concrete = Type::sum(Type::Unit, Type::Nat);

    // This should match Option because there are no free type vars
    assert_eq!(
        elab.try_match_adt_type(&sum_concrete),
        Some("Option".to_string())
    );
}

/// The problematic case: Product(TyVar("T"), α_List) inside a List μ-type.
/// This should NOT be matched as Option or any other ADT.
#[test]
fn test_try_match_adt_type_rejects_list_inner_product() {
    let mut elab = make_elaborator();
    let span = Span::new(0, 0);

    // Register Option<T>
    elab.env.define_type(TypeDef {
        name: "Option".to_string(),
        params: vec!["T".to_string()],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "None".to_string(),
                fields: vec![],
                index: 0,
                span,
            },
            Constructor {
                name: "Some".to_string(),
                fields: vec![Type::TyVar("T".to_string())],
                index: 1,
                span,
            },
        ]),
        visibility: Visibility::Public,
        span,
        defining_module: None,
        encoded_type: None,
    });

    // Product(TyVar("T"), TyVar("α_List"))
    // This is the Cons variant's payload in List<T>
    let product_with_tyvar = Type::product(
        Type::TyVar("T".to_string()),
        Type::TyVar("α_List".to_string()),
    );

    // Should NOT match any ADT because it contains TyVar("T")
    assert_eq!(elab.try_match_adt_type(&product_with_tyvar), None);
}

/// Sum(Unit, Product(TyVar("T"), α_List)) - the full List<T> body.
/// Should NOT match Option even though Sum(Unit, X) looks like it.
#[test]
fn test_try_match_adt_type_rejects_generic_list_body() {
    let mut elab = make_elaborator();
    let span = Span::new(0, 0);

    // Register Option<T>
    elab.env.define_type(TypeDef {
        name: "Option".to_string(),
        params: vec!["T".to_string()],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "None".to_string(),
                fields: vec![],
                index: 0,
                span,
            },
            Constructor {
                name: "Some".to_string(),
                fields: vec![Type::TyVar("T".to_string())],
                index: 1,
                span,
            },
        ]),
        visibility: Visibility::Public,
        span,
        defining_module: None,
        encoded_type: None,
    });

    // Sum(Unit, Product(TyVar("T"), TyVar("α_List")))
    // This is the body of List<T>'s μ-type encoding
    let list_body = Type::sum(
        Type::Unit,
        Type::product(
            Type::TyVar("T".to_string()),
            Type::TyVar("α_List".to_string()),
        ),
    );

    // Should NOT match Option - it has free TyVar("T")
    assert_eq!(elab.try_match_adt_type(&list_body), None);
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
    let info = env::ConstructorInfo {
        type_name: "NonExistent".to_string(),
        index: 0,
        arity: 0,
        defining_module: None,
    };
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
            },
            Constructor {
                name: "Some".to_string(),
                fields: vec![Type::TyVar("T".to_string())],
                index: 1,
                span,
            },
        ]),
        visibility: Visibility::Public,
        span,
        defining_module: None,
        encoded_type: None,
    });

    let info = env::ConstructorInfo {
        type_name: "Option".to_string(),
        index: 0,
        arity: 0,
        defining_module: None,
    };
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
