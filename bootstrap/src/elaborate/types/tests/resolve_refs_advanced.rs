use std::collections::HashSet;

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::types::resolve_refs::AppResolveMode;
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Type};

use super::resolve_refs::{dummy_span, make_elaborator, register_list_adt};

// ========================================================================

/// Record types should be kept as App (not encoded).
#[test]
fn test_resolve_app_to_encoding_record_keeps_app() {
    let mut elab = make_elaborator();

    elab.env.define_type(TypeDef {
        name: "Pair".to_string(),
        params: vec!["A".to_string(), "B".to_string()],
        kind: TypeDefKind::Record(vec![
            ("fst".to_string(), Type::TyVar("A".to_string())),
            ("snd".to_string(), Type::TyVar("B".to_string())),
        ]),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let mut stack = HashSet::new();
    let result = elab.resolve_app_to_encoding(
        "Pair",
        vec![Type::Nat, Type::String],
        &mut stack,
        AppResolveMode::TypeRefs,
    );

    assert!(
        matches!(&result, Type::App(name, _) if name == "Pair"),
        "Expected App(\"Pair\", ..) for record type, got {:?}",
        result
    );
}

/// Stub types should be kept as App (not encoded).
#[test]
fn test_resolve_app_to_encoding_stub_keeps_app() {
    let mut elab = make_elaborator();

    elab.env.define_type(TypeDef {
        name: "Foreign".to_string(),
        params: vec![],
        kind: TypeDefKind::Stub,
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let mut stack = HashSet::new();
    let result =
        elab.resolve_app_to_encoding("Foreign", vec![], &mut stack, AppResolveMode::TypeRefs);

    assert!(
        matches!(&result, Type::App(name, _) if name == "Foreign"),
        "Expected App(\"Foreign\", ..) for stub type, got {:?}",
        result
    );
}

/// Unknown type names should be kept as App unchanged.
#[test]
fn test_resolve_app_to_encoding_unknown_type() {
    let mut elab = make_elaborator();

    let mut stack = HashSet::new();
    let result = elab.resolve_app_to_encoding(
        "DoesNotExist",
        vec![Type::Nat],
        &mut stack,
        AppResolveMode::TypeApps,
    );

    assert!(
        matches!(&result, Type::App(name, args) if name == "DoesNotExist" && args.len() == 1),
        "Expected App(\"DoesNotExist\", [Nat]) for unknown type, got {:?}",
        result
    );
}

/// Parameterized alias (e.g. `type Wrap<T> = List<T>`) should substitute
/// params and recurse to produce a μ-type.
#[test]
fn test_resolve_app_to_encoding_parameterized_alias() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    // type Wrap<T> = List<T>
    elab.env.define_type(TypeDef {
        name: "Wrap".to_string(),
        params: vec!["T".to_string()],
        kind: TypeDefKind::Alias(Type::app("List", vec![Type::TyVar("T".to_string())])),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Wrap<Nat> should resolve through to List<Nat> → μ-type
    let mut stack = HashSet::new();
    let result = elab.resolve_app_to_encoding(
        "Wrap",
        vec![Type::Nat],
        &mut stack,
        AppResolveMode::TypeRefs,
    );

    assert!(
        matches!(&result, Type::Mu(_, _)),
        "Expected μ-type for Wrap<Nat> → List<Nat>, got {:?}",
        result
    );
}

/// Parameterized alias should produce same result in both modes.
#[test]
fn test_resolve_app_to_encoding_parameterized_alias_modes_agree() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    // type Wrap<T> = List<T>
    elab.env.define_type(TypeDef {
        name: "Wrap".to_string(),
        params: vec!["T".to_string()],
        kind: TypeDefKind::Alias(Type::app("List", vec![Type::TyVar("T".to_string())])),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let mut stack1 = HashSet::new();
    let result_refs = elab.resolve_app_to_encoding(
        "Wrap",
        vec![Type::Nat],
        &mut stack1,
        AppResolveMode::TypeRefs,
    );

    let mut stack2 = HashSet::new();
    let result_apps = elab.resolve_app_to_encoding(
        "Wrap",
        vec![Type::Nat],
        &mut stack2,
        AppResolveMode::TypeApps,
    );

    assert_eq!(
        result_refs, result_apps,
        "TypeRefs and TypeApps should agree on parameterized alias"
    );
}

/// Recursive alias should not cause infinite loop — cycle detection
/// via alias_expansion_stack should return App unchanged.
#[test]
fn test_resolve_app_to_encoding_recursive_alias_terminates() {
    let mut elab = make_elaborator();

    // type Loop = Loop  (degenerate recursive alias)
    elab.env.define_type(TypeDef {
        name: "Loop".to_string(),
        params: vec![],
        kind: TypeDefKind::Alias(Type::app("Loop", vec![])),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let mut stack = HashSet::new();
    // This must terminate (not stack overflow)
    let result = elab.resolve_app_to_encoding("Loop", vec![], &mut stack, AppResolveMode::TypeRefs);

    // The inner App("Loop") hits the cycle guard → kept as App
    // (the resolved result depends on how resolve_type_references_impl
    // handles App with name in the stack — it should resolve args only)
    assert!(
        !matches!(&result, Type::Mu(_, _)),
        "Recursive alias should not produce μ-type, got {:?}",
        result
    );
}
