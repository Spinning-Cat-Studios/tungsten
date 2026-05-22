//! Tests for type reference resolution (resolve_refs.rs).
//!
//! Covers:
//! - resolve_type_references_impl cycle detection (ADR 20.4.26b)
//! - Shared resolve_app_to_encoding branches (ADR 20.4.26h §1)
//! - Record, Stub, unknown type, parameterized alias, recursive alias

use std::collections::HashSet;

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::types::resolve_refs::AppResolveMode;
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Type};

pub(super) fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

pub(super) fn dummy_span() -> Span {
    Span::new(0, 0)
}

/// Register a standard List<T> = Nil | Cons(T, List<T>) ADT.
pub(super) fn register_list_adt(elab: &mut Elaborator<'_>) {
    elab.env.define_type(TypeDef {
        name: "List".to_string(),
        params: vec!["T".to_string()],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "Nil".to_string(),
                fields: vec![],
                index: 0,
                visibility: None,
                span: dummy_span(),
            },
            Constructor {
                name: "Cons".to_string(),
                fields: vec![
                    Type::TyVar("T".to_string()),
                    Type::TyVar("List".to_string()),
                ],
                index: 1,
                visibility: None,
                span: dummy_span(),
            },
        ]),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });
}

// ========================================================================
// Tests for resolve_type_refs_app cycle detection (ADR 20.4.26b)
// ========================================================================
//
// The fix removes pre-insertion of ADT names into alias_expansion_stack before
// encode_adt_type_impl. Previously, App("List", [Nat]) would be
// "resolved" back to App("List", [Nat]) due to false cycle detection.

/// resolve_type_references_impl should fully resolve App("List", [Nat])
/// to a Mu-type, not return it unchanged as App.
#[test]
fn test_resolve_type_refs_app_produces_mu() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    let app_ty = Type::app("List", vec![Type::Nat]);
    let mut alias_expansion_stack = HashSet::new();
    let result = elab.resolve_type_references_impl(&app_ty, &mut alias_expansion_stack);

    assert!(
        matches!(&result, Type::Mu(_, _)),
        "Expected resolve_type_references_impl to produce μ-type for ADT App, got {:?}",
        result
    );
}

/// Nested App inside a Product should also be resolved.
#[test]
fn test_resolve_type_refs_nested_app() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    // Product(Nat, App("List", [Nat]))
    let ty = Type::product(Type::Nat, Type::app("List", vec![Type::Nat]));
    let mut alias_expansion_stack = HashSet::new();
    let result = elab.resolve_type_references_impl(&ty, &mut alias_expansion_stack);

    // The second element should be a Mu type
    if let Type::Product(_, right) = &result {
        assert!(
            matches!(&**right, Type::Mu(_, _)),
            "Expected μ-type in product right, got {:?}",
            right
        );
    } else {
        panic!("Expected Product, got {:?}", result);
    }
}

/// Alias types should still be expanded (not broken by the ADT fix).
#[test]
fn test_resolve_type_refs_alias_still_works() {
    let mut elab = make_elaborator();

    elab.env.define_type(TypeDef {
        name: "MyNat".to_string(),
        params: vec![],
        kind: TypeDefKind::Alias(Type::Nat),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let app_ty = Type::app("MyNat", vec![]);
    let mut alias_expansion_stack = HashSet::new();
    let result = elab.resolve_type_references_impl(&app_ty, &mut alias_expansion_stack);

    assert_eq!(result, Type::Nat, "Alias should resolve to Nat");
}

// ========================================================================
// Tests for shared resolve_app_to_encoding (ADR 20.4.26h §1)
// ========================================================================

/// resolve_app_to_encoding with TypeApps mode should produce the same
/// μ-type as TypeRefs mode for an ADT.
#[test]
fn test_resolve_app_to_encoding_type_apps_produces_mu() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    let resolved_args = vec![Type::Nat];
    let mut stack = HashSet::new();

    let result =
        elab.resolve_app_to_encoding("List", resolved_args, &mut stack, AppResolveMode::TypeApps);

    assert!(
        matches!(&result, Type::Mu(_, _)),
        "Expected μ-type for ADT via TypeApps mode, got {:?}",
        result
    );
}

/// Both modes should produce identical results for ADT encoding.
#[test]
fn test_resolve_app_to_encoding_modes_agree_on_adt() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    let mut stack1 = HashSet::new();
    let result_refs = elab.resolve_app_to_encoding(
        "List",
        vec![Type::Nat],
        &mut stack1,
        AppResolveMode::TypeRefs,
    );

    let mut stack2 = HashSet::new();
    let result_apps = elab.resolve_app_to_encoding(
        "List",
        vec![Type::Nat],
        &mut stack2,
        AppResolveMode::TypeApps,
    );

    assert_eq!(
        result_refs, result_apps,
        "TypeRefs and TypeApps modes should produce identical ADT encodings"
    );
}

/// resolve_app_to_encoding should expand aliases in both modes.
#[test]
fn test_resolve_app_to_encoding_alias_both_modes() {
    let mut elab = make_elaborator();

    elab.env.define_type(TypeDef {
        name: "MyNat".to_string(),
        params: vec![],
        kind: TypeDefKind::Alias(Type::Nat),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let mut stack1 = HashSet::new();
    let result_refs =
        elab.resolve_app_to_encoding("MyNat", vec![], &mut stack1, AppResolveMode::TypeRefs);

    let mut stack2 = HashSet::new();
    let result_apps =
        elab.resolve_app_to_encoding("MyNat", vec![], &mut stack2, AppResolveMode::TypeApps);

    assert_eq!(result_refs, Type::Nat);
    assert_eq!(result_apps, Type::Nat);
}

// ========================================================================
// Additional coverage for resolve_app_to_encoding branches (ADR 20.4.26h)
