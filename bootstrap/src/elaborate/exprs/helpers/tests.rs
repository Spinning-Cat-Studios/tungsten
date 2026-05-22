use super::*;
use std::collections::HashMap;
use tungsten_core::Context;

/// Create an Elaborator for testing.
fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

// ========================================================================
// Tests for `extract_type_args_into_subst` (ADR 30.1.26.2 fix)
// ========================================================================
//
// The fix modifies how TyVars are handled during type argument extraction:
// - TyVars like "T" in a μ-type body ARE type arguments (not skipped)
// - Only μ-bound vars (α_*) are skipped
//
// This enables correct type inference for generic ADT patterns like List<T>

/// Extract type args from a simple instantiation: List<Nat> body.
/// Mu("α_List", Sum(Unit, Product(Nat, TyVar("α_List"))))
/// Should extract [Nat] for type param T.
#[test]
fn test_extract_type_args_list_nat() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];
    let mut subst = HashMap::new();

    // Mu("α_List", Sum(Unit, Product(Nat, TyVar("α_List"))))
    let body = Type::sum(
        Type::Unit,
        Type::product(Type::Nat, Type::TyVar("α_List".to_string())),
    );
    let list_nat = Type::mu("α_List", body);

    elab.extract_type_args_into_subst(&list_nat, &type_params, &mut subst, 0, false);

    assert_eq!(subst.get("T"), Some(&Type::Nat));
}

/// Extract type args from List<String>.
#[test]
fn test_extract_type_args_list_string() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];
    let mut subst = HashMap::new();

    // Mu("α_List", Sum(Unit, Product(String, TyVar("α_List"))))
    let body = Type::sum(
        Type::Unit,
        Type::product(Type::String, Type::TyVar("α_List".to_string())),
    );
    let list_string = Type::mu("α_List", body);

    elab.extract_type_args_into_subst(&list_string, &type_params, &mut subst, 0, false);

    assert_eq!(subst.get("T"), Some(&Type::String));
}

/// Extract type args from a generic List<T> with type variable.
/// This is the KEY TEST for the fix in ADR 30.1.26.2:
/// When we have Mu("α_List", Sum(Unit, Product(TyVar("T"), TyVar("α_List")))),
/// the TyVar("T") SHOULD be extracted as the type argument.
#[test]
fn test_extract_type_args_list_generic() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];
    let mut subst = HashMap::new();

    // Mu("α_List", Sum(Unit, Product(TyVar("T"), TyVar("α_List"))))
    // This represents List<T> from a generic context
    let body = Type::sum(
        Type::Unit,
        Type::product(
            Type::TyVar("T".to_string()), // This should be extracted!
            Type::TyVar("α_List".to_string()),
        ),
    );
    let list_t = Type::mu("α_List", body);

    elab.extract_type_args_into_subst(&list_t, &type_params, &mut subst, 0, false);

    // The fix: TyVar("T") should be extracted as the type argument
    assert_eq!(subst.get("T"), Some(&Type::TyVar("T".to_string())));
}

/// μ-bound variable (α_List) should NOT be extracted as type argument.
#[test]
fn test_extract_type_args_skips_mu_bound() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];
    let mut subst = HashMap::new();

    // Just the body: Sum(Unit, Product(TyVar("α_List"), TyVar("α_List")))
    // α_List should NOT be extracted as it's a μ-bound recursion marker
    let body = Type::sum(
        Type::Unit,
        Type::product(
            Type::TyVar("α_List".to_string()),
            Type::TyVar("α_List".to_string()),
        ),
    );

    elab.extract_type_args_into_subst(&body, &type_params, &mut subst, 1, false);

    // T should NOT be bound because α_List is not a valid type arg
    assert_eq!(subst.get("T"), None);
}

/// Extract multiple type args: Result<String, Nat>
#[test]
fn test_extract_type_args_result() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string(), "E".to_string()];
    let mut subst = HashMap::new();

    // Sum(String, Nat) - Result<String, Nat> encoded
    // Note: Result is non-recursive, so no μ-type
    let result_ty = Type::sum(Type::String, Type::Nat);

    elab.extract_type_args_into_subst(&result_ty, &type_params, &mut subst, 0, false);

    // T -> String, E -> Nat (left-to-right extraction)
    assert_eq!(subst.get("T"), Some(&Type::String));
    assert_eq!(subst.get("E"), Some(&Type::Nat));
}

/// Option<Nat>: Sum(Unit, Nat) -> extracts Nat for T
#[test]
fn test_extract_type_args_option() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];
    let mut subst = HashMap::new();

    // Sum(Unit, Nat) - Option<Nat>
    let option_nat = Type::sum(Type::Unit, Type::Nat);

    elab.extract_type_args_into_subst(&option_nat, &type_params, &mut subst, 0, false);

    // T -> Nat (Unit is skipped as not a type param instantiation)
    assert_eq!(subst.get("T"), Some(&Type::Nat));
}

/// Unit is not extracted as a type parameter.
#[test]
fn test_extract_type_args_unit_skipped() {
    let elab = make_elaborator();
    let type_params = vec!["T".to_string()];
    let mut subst = HashMap::new();

    elab.extract_type_args_into_subst(&Type::Unit, &type_params, &mut subst, 1, false);

    // Unit should not bind T
    assert_eq!(subst.get("T"), None);
}

/// Arrow types: (T -> U) should extract T and U.
#[test]
fn test_extract_type_args_arrow() {
    let elab = make_elaborator();
    let type_params = vec!["A".to_string(), "B".to_string()];
    let mut subst = HashMap::new();

    // Nat -> String
    let arrow = Type::arrow(Type::Nat, Type::String);

    elab.extract_type_args_into_subst(&arrow, &type_params, &mut subst, 0, false);

    assert_eq!(subst.get("A"), Some(&Type::Nat));
    assert_eq!(subst.get("B"), Some(&Type::String));
}

// ========================================================================
// Tests for `substitute_recursive_refs` with App adt_type (ADR 20.4.26b)
// ========================================================================
//
// The fix handles the case where adt_type is App("List", [Token]) instead
// of a Mu-encoded type. This happens when field types come from record
// definitions (Phase 1c stores App form, not μ-form).

/// Helper: register a List<T> ADT in the environment.
fn register_list_adt(elab: &mut Elaborator<'_>) {
    use crate::ast::Visibility;
    use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
    use crate::span::Span;

    let dummy_span = Span::new(0, 0);
    elab.env.define_type(TypeDef {
        name: "List".to_string(),
        params: vec!["T".to_string()],
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
                fields: vec![
                    Type::TyVar("T".to_string()),
                    Type::TyVar("List".to_string()),
                ],
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
}

/// substitute_recursive_refs with App("List", [Nat]) adt_type should
/// resolve TyVar("List") to the full μ-type, not leave it as bare "List".
#[test]
fn test_substitute_recursive_refs_app_adt_type() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    let field_ty = Type::TyVar("List".to_string()); // Cons tail field
    let adt_type = Type::app("List", vec![Type::Nat]); // App form from record

    let result = elab.substitute_recursive_refs(&field_ty, &adt_type);

    // Should produce a Mu type, not bare TyVar("List")
    assert!(
        matches!(&result, Type::Mu(_, _)),
        "Expected μ-type from App adt_type resolution, got {:?}",
        result
    );
}

/// substitute_recursive_refs with Mu adt_type should still work (existing path).
#[test]
fn test_substitute_recursive_refs_mu_adt_type() {
    let mut elab = make_elaborator();

    // Mu("α_List", Sum(Unit, Product(Nat, TyVar("α_List"))))
    let mu_type = Type::mu(
        "α_List",
        Type::sum(
            Type::Unit,
            Type::product(Type::Nat, Type::TyVar("α_List".to_string())),
        ),
    );
    let field_ty = Type::TyVar("List".to_string());

    let result = elab.substitute_recursive_refs(&field_ty, &mu_type);

    // TyVar("List") should be replaced with the full μ-type
    assert!(
        matches!(&result, Type::Mu(_, _)),
        "Expected μ-type, got {:?}",
        result
    );
}

// ========================================================================
// Tests for resolve_type_apps cycle detection (ADR 20.4.26b)
// ========================================================================
//
// The fix removes pre-insertion of ADT names into alias_expansion_stack before
// encode_adt_type_impl. ADTs handle their own cycle detection internally;
// pre-insertion caused false cycle detection and returned unresolved App.

/// resolve_type_apps should fully resolve App("List", [Nat]) to a μ-type.
#[test]
fn test_resolve_type_apps_resolves_adt() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);

    let app_ty = Type::app("List", vec![Type::Nat]);
    let result = elab.resolve_type_apps(&app_ty);

    // Should produce Mu, not leave as App
    assert!(
        matches!(&result, Type::Mu(_, _)),
        "Expected resolve_type_apps to produce μ-type, got {:?}",
        result
    );
}

/// resolve_type_apps should preserve App for non-ADT types (records).
#[test]
fn test_resolve_type_apps_preserves_record() {
    let mut elab = make_elaborator();

    use crate::ast::Visibility;
    use crate::elaborate::env::{TypeDef, TypeDefKind};
    use crate::span::Span;

    elab.env.define_type(TypeDef {
        name: "Pair".to_string(),
        params: vec!["A".to_string(), "B".to_string()],
        kind: TypeDefKind::Record(vec![
            ("fst".to_string(), Type::TyVar("A".to_string())),
            ("snd".to_string(), Type::TyVar("B".to_string())),
        ]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    let app_ty = Type::app("Pair", vec![Type::Nat, Type::String]);
    let result = elab.resolve_type_apps(&app_ty);

    // Records stay as App (not encoded to products)
    assert!(
        matches!(&result, Type::App(_, _)),
        "Expected App for record type, got {:?}",
        result
    );
}
