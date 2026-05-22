use super::*;

#[test]
fn test_collect_at_prefixed_tyvars_none() {
    let ty = Type::Nat;
    let mut results = Vec::new();
    collect_at_prefixed_tyvars(&ty, &mut results);
    assert!(results.is_empty());
}

#[test]
fn test_collect_at_prefixed_tyvars_found() {
    let ty = Type::Arrow(Box::new(Type::TyVar("@Foo".into())), Box::new(Type::Nat));
    let mut results = Vec::new();
    collect_at_prefixed_tyvars(&ty, &mut results);
    assert_eq!(results, vec!["@Foo"]);
}

#[test]
fn test_collect_at_prefixed_tyvars_nested() {
    let ty = Type::Sum(
        Box::new(Type::TyVar("@A".into())),
        Box::new(Type::Product(
            Box::new(Type::TyVar("@B".into())),
            Box::new(Type::Unit),
        )),
    );
    let mut results = Vec::new();
    collect_at_prefixed_tyvars(&ty, &mut results);
    assert_eq!(results, vec!["@A", "@B"]);
}

#[test]
fn test_collect_at_prefixed_tyvars_ignores_normal() {
    let ty = Type::TyVar("α_List".into());
    let mut results = Vec::new();
    collect_at_prefixed_tyvars(&ty, &mut results);
    assert!(results.is_empty());
}

#[test]
fn test_collect_non_mu_tyvars_bound() {
    // Mu(α, TyVar(α)) — α is bound, should not appear
    let ty = Type::Mu("α".into(), Box::new(Type::TyVar("α".into())));
    let mut bound = Vec::new();
    let mut results = Vec::new();
    collect_non_mu_tyvars(&ty, &mut bound, &mut results);
    assert!(results.is_empty());
}

#[test]
fn test_collect_non_mu_tyvars_free() {
    // Arrow(TyVar("X"), Nat) — X is free
    let ty = Type::Arrow(Box::new(Type::TyVar("X".into())), Box::new(Type::Nat));
    let mut bound = Vec::new();
    let mut results = Vec::new();
    collect_non_mu_tyvars(&ty, &mut bound, &mut results);
    assert_eq!(results, vec!["X"]);
}

#[test]
fn test_phase_display() {
    assert_eq!(format!("{}", ElaborationPhase::Phase1a), "Phase 1a");
    assert_eq!(format!("{}", ElaborationPhase::Phase1c5), "Phase 1c.5");
    assert_eq!(format!("{}", ElaborationPhase::Phase1e), "Phase 1e");
}

// ================================================================
// Violation detection tests — verify each check catches bad state
// ================================================================

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::span::Span;
use tungsten_core::Context;

/// Helper: create an Elaborator with phase checking enabled.
fn make_checking_elaborator(ctx: &mut Context) -> crate::elaborate::Elaborator<'_> {
    let mut elab = crate::elaborate::Elaborator::new(ctx);
    elab.set_check_phase_invariants(true);
    elab
}

#[test]
fn test_check_phase_1a_detects_non_stub() {
    let mut ctx = Context::new();
    let mut elab = make_checking_elaborator(&mut ctx);

    // Insert a non-stub local type (simulates Phase 1c running before 1a check)
    elab.env.define_type(TypeDef {
        name: "Sneaky".into(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    elab.check_phase_1a();

    let result = &elab.phase_invariant_results[0];
    assert!(!result.passed, "should detect non-stub local type after 1a");
    assert!(result.violations[0].contains("Sneaky"));
}

#[test]
fn test_check_phase_1d_detects_unresolved_at_tyvar() {
    let mut ctx = Context::new();
    let mut elab = make_checking_elaborator(&mut ctx);

    // Insert a type whose field references @Unknown (not a known type)
    elab.env.define_type(TypeDef {
        name: "Bad".into(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![Constructor {
            name: "MkBad".into(),
            fields: vec![Type::TyVar("@Unknown".into())],
            index: 0,
            visibility: None,
            span: Span::new(0, 0),
        }]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    elab.check_phase_1d();

    let result = &elab.phase_invariant_results[0];
    assert!(!result.passed, "should detect unresolved @Unknown TyVar");
    assert!(result.violations[0].contains("@Unknown"));
}

#[test]
fn test_check_phase_1e_detects_escape_to_unknown() {
    let mut ctx = Context::new();
    let mut elab = make_checking_elaborator(&mut ctx);

    // Insert a type with a cached encoding containing @Unknown
    elab.env.define_type(TypeDef {
        name: "Leaky".into(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: Some(Type::Arrow(
            Box::new(Type::TyVar("@Unknown".into())),
            Box::new(Type::Nat),
        )),
        field_visibilities: Vec::new(),
    });

    elab.check_phase_1e();

    let result = &elab.phase_invariant_results[0];
    assert!(
        !result.passed,
        "should detect @-TyVar escape to unknown type"
    );
    assert!(result.violations[0].contains("@Unknown"));
}

#[test]
fn test_check_phase_1e_tolerates_known_circular_ref() {
    let mut ctx = Context::new();
    let mut elab = make_checking_elaborator(&mut ctx);

    // Register both types so @B is a known cross-reference
    elab.env.define_type(TypeDef {
        name: "A".into(),
        params: vec![],
        kind: TypeDefKind::Record(vec![("b".into(), Type::TyVar("@B".into()))]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: Some(Type::TyVar("@B".into())),
        field_visibilities: Vec::new(),
    });
    elab.env.define_type(TypeDef {
        name: "B".into(),
        params: vec![],
        kind: TypeDefKind::Record(vec![("a".into(), Type::TyVar("@A".into()))]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: Some(Type::TyVar("@A".into())),
        field_visibilities: Vec::new(),
    });

    elab.check_phase_1e();

    let result = &elab.phase_invariant_results[0];
    assert!(
        result.passed,
        "should tolerate @-refs to known types (circular deps)"
    );
}

// ================================================================
// Integration test — run full elaboration with phase checks
// ================================================================

#[test]
fn test_full_elaboration_all_phases_pass() {
    let source = r#"
        type Color = Red | Green | Blue
        fn main() -> Nat { 42 }
    "#;

    let (ast, parse_errors) = crate::parse(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let mut ctx = Context::new();
    let (phase_results, elab_result) =
        crate::elaborate::elaborate_with_phase_checks(&ast, &mut ctx);

    assert!(elab_result.is_ok(), "elaboration should succeed");
    assert_eq!(phase_results.len(), 7, "should have 7 phase check results");

    for result in &phase_results {
        assert!(
            result.passed,
            "{} failed: {:?}",
            result.phase, result.violations
        );
    }
}

#[test]
fn test_constructor_metadata_check_detects_violation() {
    let mut ctx = Context::new();
    let mut elab = make_checking_elaborator(&mut ctx);

    // Register an ADT with two constructors that have conflicting indices
    // (both use index 0 — violates "unique indices" invariant)
    elab.env.define_type(TypeDef {
        name: "Bad".into(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "A".into(),
                fields: vec![],
                index: 0,
                visibility: None,
                span: Span::new(0, 0),
            },
            Constructor {
                name: "B".into(),
                fields: vec![],
                index: 0, // conflict: same index as A
                span: Span::new(0, 0),
                visibility: None,
            },
        ]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    elab.check_constructor_metadata();

    assert_eq!(elab.phase_invariant_results.len(), 1);
    let result = &elab.phase_invariant_results[0];
    assert!(
        !result.passed,
        "should detect conflicting constructor indices"
    );
    assert!(
        !result.violations.is_empty(),
        "should have at least one violation"
    );
    // Two constructors sharing index 0 → DuplicateIndex and NonContiguousIndices
    let violations_text = format!("{:?}", result.violations);
    assert!(
        violations_text.contains("DuplicateIndex")
            || violations_text.contains("NonContiguousIndices"),
        "violation should be DuplicateIndex or NonContiguousIndices, got: {violations_text}"
    );
}
