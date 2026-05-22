//! Cycle detection tests for ADT encoding.
//!
//! These tests verify that the type checker terminates on mutually recursive
//! types, which would previously cause infinite loops. The fix involves passing
//! a mu_encoding_stack through all type encoding functions to detect cycles.
//!
//! See ADR 25.1.26.Tungsten-Type-Checker-Totality.md for background.

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Type};

fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

#[test]
fn test_encode_adt_mutually_recursive_types_terminates() {
    // Test case: Mutually recursive ADTs
    // type A = MkA(B)
    // type B = MkB(A)
    //
    // This previously caused infinite loop in encode_adt_type because each
    // function created a fresh mu_encoding_stack.

    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    // Define ADT A with constructor MkA(B)
    elab.env.define_type(TypeDef {
        name: "A".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![Constructor {
            name: "MkA".to_string(),
            fields: vec![Type::app("B", vec![])],
            index: 0,
            visibility: None,
            span: dummy_span,
        }]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // Define ADT B with constructor MkB(A)
    elab.env.define_type(TypeDef {
        name: "B".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![Constructor {
            name: "MkB".to_string(),
            fields: vec![Type::app("A", vec![])],
            index: 0,
            visibility: None,
            span: dummy_span,
        }]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // This should terminate (not hang) due to cycle detection
    let result = elab.encode_adt_type("A", &[]);

    // Verify it terminates - if we get here, cycle detection worked!
    assert!(
        result.is_ok(),
        "encode_adt_type should succeed: {:?}",
        result
    );
}

#[test]
fn test_encode_adt_three_way_mutual_recursion_terminates() {
    // Test case: Three-way mutually recursive ADTs
    // type A = MkA(B)
    // type B = MkB(C)
    // type C = MkC(A)

    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    elab.env.define_type(TypeDef {
        name: "A".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![Constructor {
            name: "MkA".to_string(),
            fields: vec![Type::app("B", vec![])],
            index: 0,
            visibility: None,
            span: dummy_span,
        }]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    elab.env.define_type(TypeDef {
        name: "B".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![Constructor {
            name: "MkB".to_string(),
            fields: vec![Type::app("C", vec![])],
            index: 0,
            visibility: None,
            span: dummy_span,
        }]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    elab.env.define_type(TypeDef {
        name: "C".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![Constructor {
            name: "MkC".to_string(),
            fields: vec![Type::app("A", vec![])],
            index: 0,
            visibility: None,
            span: dummy_span,
        }]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });

    // This should terminate
    let result = elab.encode_adt_type("A", &[]);
    assert!(result.is_ok());
}

#[test]
fn test_encode_adt_self_recursive_terminates() {
    // Test case: Self-recursive ADT (simpler case, but should still work)
    // type List<T> = Nil | Cons(T, List<T>)

    let mut elab = make_elaborator();
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
                    Type::app("List", vec![Type::TyVar("T".to_string())]),
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

    let result = elab.encode_adt_type("List", &[Type::Nat]);
    assert!(result.is_ok());

    // The result should be a Mu type (recursive type)
    let encoded = result.unwrap();
    assert!(
        matches!(&encoded, Type::Mu(..)),
        "Expected Mu type for recursive ADT, got {:?}",
        encoded
    );
}

#[test]
fn test_encode_adt_multiple_constructors_with_mutual_recursion() {
    // Test case: More complex mutual recursion with multiple constructors
    // type Expr = Var(String) | App(Expr, Expr) | TypeAnn(Expr, TypeExpr)
    // type TypeExpr = TyVar(String) | TyEq(Expr, Expr)
    //
    // This is based on the actual Tungsten AST that revealed the bug.

    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    elab.env.define_type(TypeDef {
        name: "Expr".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "Var".to_string(),
                fields: vec![Type::String],
                index: 0,
                visibility: None,
                span: dummy_span,
            },
            Constructor {
                name: "App".to_string(),
                fields: vec![Type::app("Expr", vec![]), Type::app("Expr", vec![])],
                index: 1,
                visibility: None,
                span: dummy_span,
            },
            Constructor {
                name: "TypeAnn".to_string(),
                fields: vec![Type::app("Expr", vec![]), Type::app("TypeExpr", vec![])],
                index: 2,
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

    elab.env.define_type(TypeDef {
        name: "TypeExpr".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "TyVar".to_string(),
                fields: vec![Type::String],
                index: 0,
                visibility: None,
                span: dummy_span,
            },
            Constructor {
                name: "TyEq".to_string(),
                fields: vec![Type::app("Expr", vec![]), Type::app("Expr", vec![])],
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

    // Both should terminate
    let expr_result = elab.encode_adt_type("Expr", &[]);
    assert!(
        expr_result.is_ok(),
        "Encoding Expr should succeed: {:?}",
        expr_result
    );

    let type_expr_result = elab.encode_adt_type("TypeExpr", &[]);
    assert!(
        type_expr_result.is_ok(),
        "Encoding TypeExpr should succeed: {:?}",
        type_expr_result
    );
}
