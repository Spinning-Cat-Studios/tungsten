//! Basic type elaboration tests.

use crate::ast::{Ident, Path, TypeExpr};
use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Type};

fn make_elaborator() -> Elaborator<'static> {
    // We need a static reference, so we use a leaked context
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

#[test]
fn test_elab_nat() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Path(Path::simple(Ident::new("Nat", Span::new(0, 3))));
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::Nat);
}

#[test]
fn test_elab_bool() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Path(Path::simple(Ident::new("Bool", Span::new(0, 4))));
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::Bool);
}

#[test]
fn test_elab_unit() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Unit(Span::new(0, 4));
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::Unit);
}

#[test]
fn test_elab_void() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Void(Span::new(0, 4));
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::Void);
}

#[test]
fn test_elab_arrow() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Arrow(
        Box::new(TypeExpr::Path(Path::simple(Ident::new(
            "Nat",
            Span::new(0, 3),
        )))),
        Box::new(TypeExpr::Path(Path::simple(Ident::new(
            "Bool",
            Span::new(7, 11),
        )))),
        Span::new(0, 11),
    );
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::arrow(Type::Nat, Type::Bool));
}

#[test]
fn test_elab_product() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Product(
        Box::new(TypeExpr::Path(Path::simple(Ident::new(
            "Nat",
            Span::new(0, 3),
        )))),
        Box::new(TypeExpr::Path(Path::simple(Ident::new(
            "Bool",
            Span::new(6, 10),
        )))),
        Span::new(0, 10),
    );
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::product(Type::Nat, Type::Bool));
}

#[test]
fn test_elab_sum() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Sum(
        Box::new(TypeExpr::Path(Path::simple(Ident::new(
            "Nat",
            Span::new(0, 3),
        )))),
        Box::new(TypeExpr::Path(Path::simple(Ident::new(
            "Bool",
            Span::new(6, 10),
        )))),
        Span::new(0, 10),
    );
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::sum(Type::Nat, Type::Bool));
}

#[test]
fn test_elab_forall() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Forall(
        Ident::new("T", Span::new(7, 8)),
        Box::new(TypeExpr::Arrow(
            Box::new(TypeExpr::Path(Path::simple(Ident::new(
                "T",
                Span::new(10, 11),
            )))),
            Box::new(TypeExpr::Path(Path::simple(Ident::new(
                "T",
                Span::new(15, 16),
            )))),
            Span::new(10, 16),
        )),
        Span::new(0, 16),
    );
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(
        result,
        Type::forall(
            "T",
            Type::arrow(Type::TyVar("T".to_string()), Type::TyVar("T".to_string()))
        )
    );
}

#[test]
fn test_elab_type_var_in_scope() {
    let mut elab = make_elaborator();
    elab.env.push_type_var("T".to_string());

    let ty = TypeExpr::Path(Path::simple(Ident::new("T", Span::new(0, 1))));
    let result = elab.elab_type(&ty).unwrap();
    assert_eq!(result, Type::TyVar("T".to_string()));
}

#[test]
fn test_elab_undefined_type() {
    let mut elab = make_elaborator();
    let ty = TypeExpr::Path(Path::simple(Ident::new("Undefined", Span::new(0, 9))));
    let result = elab.elab_type(&ty);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err.kind, ElabErrorKind::UndefinedType(_)));
}
