//! Discovery tests: find TyApp(Global(name), ty_arg) in term trees.

use std::path::PathBuf;

use tungsten_core::terms::Term;
use tungsten_core::types::Type;

use crate::compile::mono::*;

use super::{make_def, make_unit};

#[test]
fn test_discover_ty_app_global() {
    let term = Term::ty_app(Term::Global("f".into()), Type::Nat);
    let unit = make_unit(
        &["alpha"],
        "alpha.tg",
        vec![make_def("caller", term, Type::Nat)],
    );
    let f_unit = make_unit(
        &["beta"],
        "beta.tg",
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
    let keys = table.unique_keys();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].def_id.name, "f");
}

#[test]
fn test_discover_same_generic_two_modules() {
    let term = Term::ty_app(Term::Global("f".into()), Type::Nat);
    let alpha = make_unit(
        &["alpha"],
        "alpha.tg",
        vec![make_def("use_f_alpha", term.clone(), Type::Nat)],
    );
    let beta = make_unit(
        &["beta"],
        "beta.tg",
        vec![make_def("use_f_beta", term, Type::Nat)],
    );
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
        &[alpha, beta, f_unit],
        std::path::Path::new(""),
        &Default::default(),
    );
    assert_eq!(table.requests().len(), 2);
    assert_eq!(table.unique_keys().len(), 1);
}

#[test]
fn test_discover_different_instantiations() {
    let t_nat = Term::ty_app(Term::Global("f".into()), Type::Nat);
    let t_bool = Term::ty_app(Term::Global("f".into()), Type::Bool);
    let unit = make_unit(
        &["caller"],
        "caller.tg",
        vec![
            make_def("use_nat", t_nat, Type::Nat),
            make_def("use_bool", t_bool, Type::Bool),
        ],
    );
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
    assert_eq!(table.unique_keys().len(), 2);
}

#[test]
fn test_discover_nested_ty_app_multi_param() {
    let inner_app = Term::ty_app(Term::Global("f".into()), Type::Nat);
    let outer_app = Term::TyApp(Box::new(inner_app), Type::Bool);

    let unit = make_unit(
        &["caller"],
        "caller.tg",
        vec![make_def("use_f", outer_app, Type::Nat)],
    );
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![make_def(
            "f",
            Term::TyAbs(
                "A".into(),
                Box::new(Term::TyAbs("B".into(), Box::new(Term::Unit))),
            ),
            Type::Forall(
                "A".into(),
                Box::new(Type::Forall("B".into(), Box::new(Type::Unit))),
            ),
        )],
    );

    let table = discover_mono_requests(
        &[unit, f_unit],
        std::path::Path::new(""),
        &Default::default(),
    );
    let keys = table.unique_keys();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].def_id.name, "f");
    // Multi-type-arg: TyApp(TyApp(Global("f"), Nat), Bool) → f<Nat, Bool>
    assert_eq!(
        keys[0].type_args,
        CanonicalTypeArgs::from_types(&[Type::Nat, Type::Bool])
    );
}

#[test]
fn test_discover_ty_app_inside_lambda() {
    let body = Term::ty_app(Term::Global("f".into()), Type::Bool);
    let term = Term::Lambda("x".into(), Type::Nat, Box::new(body));

    let unit = make_unit(
        &["caller"],
        "caller.tg",
        vec![make_def("wrapper", term, Type::Nat)],
    );
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
    let keys = table.unique_keys();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].def_id.name, "f");
    assert_eq!(keys[0].type_args, CanonicalTypeArgs::from_type(&Type::Bool));
}

#[test]
fn test_discover_ty_app_inside_let() {
    let val = Term::ty_app(Term::Global("f".into()), Type::Nat);
    let term = Term::Let(
        "x".into(),
        Type::Nat,
        Box::new(val),
        Box::new(Term::Var("x".into())),
    );

    let unit = make_unit(
        &["caller"],
        "caller.tg",
        vec![make_def("wrapper", term, Type::Nat)],
    );
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
    let keys = table.unique_keys();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].def_id.name, "f");
}

#[test]
fn test_discover_unknown_global_produces_no_request() {
    let term = Term::ty_app(Term::Global("nonexistent".into()), Type::Nat);
    let unit = make_unit(
        &["caller"],
        "caller.tg",
        vec![make_def("use_missing", term, Type::Nat)],
    );

    let table = discover_mono_requests(&[unit], std::path::Path::new(""), &Default::default());
    assert_eq!(table.unique_keys().len(), 0);
    assert_eq!(table.requests().len(), 0);
}

/// A polymorphic function body containing another TyApp should be discoverable
/// when the outer function is itself a mono request target. This tests that
/// discovery walks into TyAbs bodies (which it does, since TyAbs is a single-child
/// term node).
#[test]
fn test_discover_nested_mono_inside_tyabs_body() {
    // f<T> = g<Nat> (polymorphic f body calls g<Nat>)
    let g_call = Term::ty_app(Term::Global("g".into()), Type::Nat);
    let f_body = Term::TyAbs("T".into(), Box::new(g_call));

    // caller uses f<Bool>
    let caller_term = Term::ty_app(Term::Global("f".into()), Type::Bool);

    let caller_unit = make_unit(
        &["caller"],
        "caller.tg",
        vec![make_def("use_f", caller_term, Type::Nat)],
    );
    let f_unit = make_unit(
        &["lib"],
        "lib.tg",
        vec![
            make_def("f", f_body, Type::Forall("T".into(), Box::new(Type::Unit))),
            make_def(
                "g",
                Term::TyAbs("U".into(), Box::new(Term::Unit)),
                Type::Forall("U".into(), Box::new(Type::Unit)),
            ),
        ],
    );

    let table = discover_mono_requests(
        &[caller_unit, f_unit],
        std::path::Path::new(""),
        &Default::default(),
    );
    let keys = table.unique_keys();
    // Should find both f<Bool> and g<Nat> (g<Nat> is in f's TyAbs body)
    assert_eq!(
        keys.len(),
        2,
        "should discover both f<Bool> and g<Nat>, got: {:?}",
        keys
    );
    let names: Vec<&str> = keys.iter().map(|k| k.def_id.name.as_str()).collect();
    assert!(names.contains(&"f"), "should discover f<Bool>");
    assert!(names.contains(&"g"), "should discover g<Nat>");
}
