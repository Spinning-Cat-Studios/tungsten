//! Phase 3 tests: `sym`, `trans`, `cong` derived combinators.

use super::*;
use crate::ast::{Ident, LambdaParam, Path, Pattern};

/// Build an annotated identity lambda `(|x| x : Nat -> Nat)`.
fn annotated_identity_nat() -> Expr {
    let x_ident = Ident {
        name: "x".into(),
        span: Span::new(1, 2),
    };
    let x_path = Path {
        segments: vec![x_ident.clone()],
        span: Span::new(4, 5),
    };
    Expr::Annot(
        Box::new(Expr::Lambda(
            vec![LambdaParam {
                pattern: Pattern::Var(x_ident),
                ty: None,
                span: Span::new(1, 2),
            }],
            Box::new(Expr::Path(x_path)),
            Span::new(0, 6),
        )),
        TypeExpr::Arrow(
            Box::new(TypeExpr::Path(Path {
                segments: vec![Ident {
                    name: "Nat".into(),
                    span: Span::new(0, 3),
                }],
                span: Span::new(0, 3),
            })),
            Box::new(TypeExpr::Path(Path {
                segments: vec![Ident {
                    name: "Nat".into(),
                    span: Span::new(0, 3),
                }],
                span: Span::new(0, 3),
            })),
            Span::new(0, 10),
        ),
        Span::new(0, 15),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// sym
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_sym_zero_eq_zero() {
    let proof = annotated_refl_zero_eq_zero();
    let sym_expr = Expr::Sym(Box::new(proof), Span::new(0, 20));
    let result = infer_expr(&sym_expr);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    let (_, ty) = result.unwrap();
    assert_eq!(ty, Type::eq(Type::Nat, Term::Zero, Term::Zero));
}

#[test]
fn test_sym_not_equality() {
    let proof = Expr::BoolLiteral(true, Span::new(0, 4));
    let sym_expr = Expr::Sym(Box::new(proof), Span::new(0, 10));
    let result = infer_expr(&sym_expr);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ElabErrorKind::SubstExpectedEquality(_)),
        "expected SubstExpectedEquality"
    );
}

#[test]
fn test_sym_parser_round_trip() {
    let body = extract_body_expr("fn test() { sym(refl) }");
    assert!(
        matches!(body, Expr::Sym(_, _)),
        "expected Expr::Sym, got {:?}",
        body
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// trans
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_trans_zero_eq_zero() {
    let h1 = annotated_refl_zero_eq_zero();
    let h2 = annotated_refl_zero_eq_zero();
    let trans_expr = Expr::Trans(Box::new(h1), Box::new(h2), Span::new(0, 30));
    let result = infer_expr(&trans_expr);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    let (_, ty) = result.unwrap();
    assert_eq!(ty, Type::eq(Type::Nat, Term::Zero, Term::Zero));
}

#[test]
fn test_trans_not_equality() {
    let h1 = Expr::BoolLiteral(true, Span::new(0, 4));
    let h2 = annotated_refl_zero_eq_zero();
    let trans_expr = Expr::Trans(Box::new(h1), Box::new(h2), Span::new(0, 20));
    let result = infer_expr(&trans_expr);
    assert!(result.is_err());
}

#[test]
fn test_trans_endpoint_mismatch() {
    // h1 : Eq(Nat, 0, 0), h2 : Eq(Nat, 1, 1) — h1's right (0) ≠ h2's left (1)
    let h1 = annotated_refl_zero_eq_zero();
    let h2 = annotated_refl_one_eq_one();
    let trans_expr = Expr::Trans(Box::new(h1), Box::new(h2), Span::new(0, 30));
    let result = infer_expr(&trans_expr);
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            ElabErrorKind::TransEndpointMismatch { .. }
        ),
        "expected TransEndpointMismatch"
    );
}

#[test]
fn test_trans_parser_round_trip() {
    let body = extract_body_expr("fn test() { trans(refl, refl) }");
    assert!(matches!(body, Expr::Trans(_, _, _)), "expected Trans");
}

// ─────────────────────────────────────────────────────────────────────────────
// cong
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_cong_succ_zero_eq_zero() {
    let f_expr = annotated_identity_nat();
    let proof = annotated_refl_zero_eq_zero();
    let cong_expr = Expr::Cong(Box::new(f_expr), Box::new(proof), Span::new(0, 30));

    let ctx = Box::leak(Box::new(Context::new()));
    let mut elab = Elaborator::new(ctx);
    let result = elab.infer(&cong_expr);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    let (_, ty) = result.unwrap();
    assert!(
        matches!(ty, Type::Eq(_, _, _)),
        "expected Eq type, got {:?}",
        ty
    );
}

#[test]
fn test_cong_not_function() {
    let f = Expr::IntLiteral(42, Span::new(0, 2));
    let proof = annotated_refl_zero_eq_zero();
    let cong_expr = Expr::Cong(Box::new(f), Box::new(proof), Span::new(0, 20));
    let result = infer_expr(&cong_expr);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ElabErrorKind::CongExpectedFunction(_)),
        "expected CongExpectedFunction"
    );
}

#[test]
fn test_cong_nested() {
    // cong(f, cong(f, refl)) — verify combinator composition
    let f = annotated_identity_nat();
    let proof = annotated_refl_zero_eq_zero();
    let inner = Expr::Cong(Box::new(f.clone()), Box::new(proof), Span::new(0, 20));
    let outer = Expr::Cong(Box::new(f), Box::new(inner), Span::new(0, 30));
    let result = infer_expr(&outer);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    let (_, ty) = result.unwrap();
    assert!(
        matches!(ty, Type::Eq(_, _, _)),
        "expected Eq type, got {:?}",
        ty
    );
}

#[test]
fn test_cong_parser_round_trip() {
    let body = extract_body_expr("fn test() { cong(refl, refl) }");
    assert!(matches!(body, Expr::Cong(_, _, _)), "expected Cong");
}
