//! Tests for constructor elaboration.

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
