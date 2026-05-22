//! Tests for equality type surface syntax (ADR 21.5.26d, Phases 1–3).
//!
//! Early tests construct AST + types directly. Tests using `Eq<T, a, b>`
//! syntax (ADR 21.5.26f) can now use `elab_ok`/`elab_err` source strings.

mod combinators;
mod eq_type_position;
mod natind;
mod refl;
mod subst;

use crate::ast::{Expr, TypeExpr};
use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Term, Type};

/// Build a `TypeExpr::Eq` for `0 == 0` (Nat equality).
fn type_expr_zero_eq_zero() -> TypeExpr {
    TypeExpr::Eq(
        Box::new(Expr::IntLiteral(0, Span::new(0, 1))),
        Box::new(Expr::IntLiteral(0, Span::new(0, 1))),
        Span::new(0, 5),
    )
}

/// Build `(refl : (0 == 0))` — an annotated refl that can be inferred.
fn annotated_refl_zero_eq_zero() -> Expr {
    Expr::Annot(
        Box::new(Expr::Refl(Span::new(0, 4))),
        type_expr_zero_eq_zero(),
        Span::new(0, 15),
    )
}

/// Build `(refl : (1 == 1))` — annotated refl for Succ(Zero).
fn annotated_refl_one_eq_one() -> Expr {
    Expr::Annot(
        Box::new(Expr::Refl(Span::new(0, 4))),
        TypeExpr::Eq(
            Box::new(Expr::IntLiteral(1, Span::new(0, 1))),
            Box::new(Expr::IntLiteral(1, Span::new(0, 1))),
            Span::new(0, 5),
        ),
        Span::new(0, 15),
    )
}

/// Helper: create an elaborator and check `refl` against `expected`.
fn check_refl(expected: &Type) -> Result<Term, ElabErrorKind> {
    let ctx = Box::leak(Box::new(Context::new()));
    let mut elab = Elaborator::new(ctx);
    let refl_expr = Expr::Refl(Span::new(0, 4));
    match elab.check(&refl_expr, expected) {
        Ok(term) => Ok(term),
        Err(e) => Err(e.kind),
    }
}

/// Helper: infer the type of an expression via the elaborator.
fn infer_expr(expr: &Expr) -> Result<(Term, Type), ElabErrorKind> {
    let ctx = Box::leak(Box::new(Context::new()));
    let mut elab = Elaborator::new(ctx);
    match elab.infer(expr) {
        Ok(result) => Ok(result),
        Err(e) => Err(e.kind),
    }
}

/// Helper: extract body expression from a parsed function.
fn extract_body_expr(source: &str) -> Expr {
    use crate::ast::{Item, Stmt};
    use crate::parser::parse;
    let (file, errors) = parse(source);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let func = match &file.items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    match &func.body {
        Expr::Block(stmts, final_expr, _) => {
            if let Some(e) = final_expr {
                (**e).clone()
            } else if let [Stmt::Expr(e, _)] = stmts.as_slice() {
                e.clone()
            } else {
                panic!("expected expression in block")
            }
        }
        other => other.clone(),
    }
}
