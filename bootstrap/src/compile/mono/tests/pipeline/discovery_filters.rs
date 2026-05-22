//! Discovery filter tests: TyVar passthrough, @-prefix, and μ-bound variable handling.

use std::path::PathBuf;

use tungsten_core::terms::Term;
use tungsten_core::types::Type;

use crate::compile::mono::*;

use super::{make_def, make_unit};

/// Type-parameter pass-through: f<T> = g<T> should NOT record g<TyVar("T")>
/// as a mono request. TyVar type args indicate a call inside a polymorphic body
/// that will be instantiated at concrete types when the outer function is
/// monomorphized. Compiling them as standalone mono instances causes
/// lower_type recursion failures.
#[test]
fn test_discovery_tyvar_passthrough_is_filtered() {
    let def = make_def(
        "f",
        Term::TyAbs(
            "T".into(),
            Box::new(Term::TyApp(
                Box::new(Term::Global("g".into())),
                Type::TyVar("T".into()),
            )),
        ),
        Type::Nat,
    );

    let unit = make_unit(
        &["m"],
        "m.tg",
        vec![def, make_def("g", Term::Unit, Type::Nat)],
    );

    let table =
        discovery::discover_mono_requests(&[unit], &PathBuf::from("."), &Default::default());

    assert_eq!(
        table.unique_keys().len(),
        0,
        "TyVar passthrough should be filtered out, not recorded as a mono request"
    );
}

/// Compound types containing TyVars (e.g., Arrow(Nat, TyVar("T"))) should
/// also be filtered out — they are not fully concrete.
#[test]
fn test_discovery_compound_tyvar_filtered() {
    let def = make_def(
        "f",
        Term::TyAbs(
            "T".into(),
            Box::new(Term::TyApp(
                Box::new(Term::Global("g".into())),
                Type::arrow(Type::Nat, Type::TyVar("T".into())),
            )),
        ),
        Type::Nat,
    );

    let unit = make_unit(
        &["m"],
        "m.tg",
        vec![def, make_def("g", Term::Unit, Type::Nat)],
    );

    let table =
        discovery::discover_mono_requests(&[unit], &PathBuf::from("."), &Default::default());

    assert_eq!(
        table.unique_keys().len(),
        0,
        "compound type with TyVar should be filtered"
    );
}

/// strip_at_prefixes should recursively strip @ from TyVars in nested types.
#[test]
fn test_strip_at_prefixes_nested_compound() {
    let ty = Type::Arrow(
        Box::new(Type::TyVar("@Token".into())),
        Box::new(Type::Product(
            Box::new(Type::TyVar("@Span".into())),
            Box::new(Type::Nat),
        )),
    );
    let stripped = discovery::strip_at_prefixes(&ty);
    let expected = Type::Arrow(
        Box::new(Type::TyVar("Token".into())),
        Box::new(Type::Product(
            Box::new(Type::TyVar("Span".into())),
            Box::new(Type::Nat),
        )),
    );
    assert_eq!(stripped, expected);
}

/// strip_at_prefixes should NOT strip α_-prefixed TyVars — those are
/// Mu-bound variables, not Phase 1c artifacts.
#[test]
fn test_strip_at_prefixes_preserves_alpha_prefix() {
    let ty = Type::Mu(
        "α_List".into(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::TyVar("α_List".into())),
        )),
    );
    let stripped = discovery::strip_at_prefixes(&ty);
    // α_List should be unchanged — strip only targets @
    assert_eq!(stripped, ty);
}

/// @-prefixed TyVars are Phase 1c refs to concrete named types.
/// They should NOT be treated as abstract type variables.
#[test]
fn test_discover_at_prefixed_tyvar_is_concrete() {
    // TyApp(Global("f"), TyVar("@Token")) — should be discovered
    let term = Term::ty_app(Term::Global("f".into()), Type::TyVar("@Token".into()));
    let unit = make_unit(&["m"], "m.tg", vec![make_def("caller", term, Type::Nat)]);
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![make_def(
            "f",
            Term::TyAbs("T".into(), Box::new(Term::Unit)),
            Type::Forall("T".into(), Box::new(Type::Unit)),
        )],
    );

    let table = discover_mono_requests(
        &[unit, f_unit],
        std::path::Path::new(""),
        &Default::default(),
    );
    assert_eq!(
        table.unique_keys().len(),
        1,
        "@-prefixed TyVar should be treated as concrete"
    );
    // Key should have @ stripped
    let key = &table.unique_keys()[0];
    assert!(
        !key.type_args.0.contains("@Token"),
        "canonical type args should have @ stripped: {}",
        key.type_args
    );
}

/// Mu-bound variables (α_-prefixed) should NOT be treated as type variables.
#[test]
fn test_discover_mu_bound_tyvar_is_not_abstract() {
    // TyApp(Global("f"), Mu("α_List", Adt(..., TyVar("α_List"))))
    // α_List is bound by the Mu — this is a concrete recursive type.
    let mu_type = Type::Mu(
        "α_List".into(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_List".into())),
            )),
        )),
    );
    let term = Term::ty_app(Term::Global("f".into()), mu_type);
    let unit = make_unit(&["m"], "m.tg", vec![make_def("caller", term, Type::Nat)]);
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![make_def(
            "f",
            Term::TyAbs("T".into(), Box::new(Term::Unit)),
            Type::Forall("T".into(), Box::new(Type::Unit)),
        )],
    );

    let table = discover_mono_requests(
        &[unit, f_unit],
        std::path::Path::new(""),
        &Default::default(),
    );
    assert_eq!(
        table.unique_keys().len(),
        1,
        "Mu-bound α_-prefixed TyVar should not block discovery"
    );
}

/// TyVars matching known concrete type names (ADTs/records) should NOT be
/// treated as abstract type variables. This is the fix for ADR 13.5.26a:
/// cross-module type references like TyVar("Binding") are concrete types,
/// not generic parameters.
#[test]
fn test_discover_concrete_named_tyvar_not_filtered() {
    use std::collections::HashSet;

    let term = Term::ty_app(Term::Global("f".into()), Type::TyVar("Token".into()));
    let unit = make_unit(&["m"], "m.tg", vec![make_def("caller", term, Type::Nat)]);
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![make_def(
            "f",
            Term::TyAbs("T".into(), Box::new(Term::Unit)),
            Type::Forall("T".into(), Box::new(Type::Unit)),
        )],
    );

    let concrete: HashSet<String> = ["Token".to_string()].into_iter().collect();
    let table = discover_mono_requests(&[unit, f_unit], std::path::Path::new(""), &concrete);
    assert_eq!(
        table.unique_keys().len(),
        1,
        "TyVar matching a concrete type name should be treated as concrete"
    );
}

/// TyVars NOT in the concrete type set should still be filtered.
#[test]
fn test_discover_unknown_tyvar_still_filtered() {
    use std::collections::HashSet;

    let term = Term::ty_app(Term::Global("f".into()), Type::TyVar("T".into()));
    let unit = make_unit(&["m"], "m.tg", vec![make_def("caller", term, Type::Nat)]);
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![make_def(
            "f",
            Term::TyAbs("T".into(), Box::new(Term::Unit)),
            Type::Forall("T".into(), Box::new(Type::Unit)),
        )],
    );

    let concrete: HashSet<String> = ["Token".to_string()].into_iter().collect();
    let table = discover_mono_requests(&[unit, f_unit], std::path::Path::new(""), &concrete);
    assert_eq!(
        table.unique_keys().len(),
        0,
        "TyVar not in concrete set should still be filtered"
    );
}

/// Compound types with TyVars matching concrete type names should be
/// discovered (e.g., Product(TyVar("ModulePath"), TyVar("ModuleItemNames"))).
#[test]
fn test_discover_compound_concrete_tyvars_not_filtered() {
    use std::collections::HashSet;

    let compound_ty = Type::product(
        Type::TyVar("ModulePath".into()),
        Type::TyVar("ModuleItemNames".into()),
    );
    let term = Term::ty_app(Term::Global("f".into()), compound_ty);
    let unit = make_unit(&["m"], "m.tg", vec![make_def("caller", term, Type::Nat)]);
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![make_def(
            "f",
            Term::TyAbs("T".into(), Box::new(Term::Unit)),
            Type::Forall("T".into(), Box::new(Type::Unit)),
        )],
    );

    let concrete: HashSet<String> = ["ModulePath".to_string(), "ModuleItemNames".to_string()]
        .into_iter()
        .collect();
    let table = discover_mono_requests(&[unit, f_unit], std::path::Path::new(""), &concrete);
    assert_eq!(
        table.unique_keys().len(),
        1,
        "compound type with all-concrete TyVars should be discovered"
    );
}

/// Compound type with a mix of concrete and abstract TyVars should be
/// filtered — the abstract TyVar makes it non-monomorphizable.
/// e.g., Product(TyVar("Token"), TyVar("T")) where only "Token" is concrete.
#[test]
fn test_discover_mixed_concrete_abstract_tyvar_filtered() {
    use std::collections::HashSet;

    let mixed_ty = Type::product(Type::TyVar("Token".into()), Type::TyVar("T".into()));
    let term = Term::ty_app(Term::Global("f".into()), mixed_ty);
    let unit = make_unit(&["m"], "m.tg", vec![make_def("caller", term, Type::Nat)]);
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![make_def(
            "f",
            Term::TyAbs("T".into(), Box::new(Term::Unit)),
            Type::Forall("T".into(), Box::new(Type::Unit)),
        )],
    );

    let concrete: HashSet<String> = ["Token".to_string()].into_iter().collect();
    let table = discover_mono_requests(&[unit, f_unit], std::path::Path::new(""), &concrete);
    assert_eq!(
        table.unique_keys().len(),
        0,
        "compound type with mix of concrete + abstract TyVars should be filtered"
    );
}
