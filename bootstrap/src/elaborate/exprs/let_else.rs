//! `let`-`else` desugaring (ADR 13.5.26f).
//!
//! Desugars `let P = expr else diverge; body` into:
//!   `match expr { P => body, _ => diverge }`
//!
//! The `else` branch must diverge (type ⊥).

use crate::ast::{self, Expr, MatchArm, Pattern};
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

use super::blocks::StmtLetCont;

/// Bundled arguments for a `let`-`else` expression.
pub(super) struct LetElseArgs<'e> {
    pub pattern: &'e Pattern,
    pub ty_ann: Option<&'e ast::TypeExpr>,
    pub value: &'e Expr,
    pub else_expr: &'e Expr,
}

impl<'a> Elaborator<'a> {
    /// Elaborate `let P = expr else diverge; body` (expression form).
    ///
    /// Desugars to: `match expr { P => body, _ => diverge }`
    pub(super) fn elab_let_else(
        &mut self,
        args: LetElseArgs<'_>,
        body: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        self.warn_if_irrefutable(args.pattern, span);
        let match_expr = build_let_else_match(args.pattern, args.value, args.else_expr, body, span);
        if let Some(expected) = expected {
            let term = self.check(&match_expr, expected)?;
            Ok((term, expected.clone()))
        } else {
            self.infer(&match_expr)
        }
    }

    /// Elaborate `let P = expr else diverge;` in block-statement position.
    ///
    /// The body is the remaining statements + final expression in the block.
    pub(super) fn elab_stmt_let_else(
        &mut self,
        args: LetElseArgs<'_>,
        cont: StmtLetCont,
        stmt_span: Span,
    ) -> ElabResult<(Term, Type)> {
        self.warn_if_irrefutable(args.pattern, stmt_span);
        let body = stmts_to_body_expr(cont.rest, cont.final_expr, cont.span);
        let match_expr =
            build_let_else_match(args.pattern, args.value, args.else_expr, &body, stmt_span);
        if let Some(expected) = cont.expected {
            let term = self.check(&match_expr, expected)?;
            Ok((term, expected.clone()))
        } else {
            self.infer(&match_expr)
        }
    }

    /// Emit W0003 if the pattern is irrefutable (variable or wildcard).
    fn warn_if_irrefutable(&mut self, pattern: &Pattern, span: Span) {
        if matches!(pattern, Pattern::Var(_) | Pattern::Wildcard(_)) {
            self.warn(ElabError::new(span, ElabErrorKind::LetElseIrrefutable));
        }
    }
}

/// Build the desugared match expression:
/// `match value { pattern => body, _ => else_expr }`
fn build_let_else_match(
    pattern: &Pattern,
    value: &Expr,
    else_expr: &Expr,
    body: &Expr,
    span: Span,
) -> Expr {
    let success_arm = MatchArm {
        pattern: pattern.clone(),
        guard: None,
        body: body.clone(),
        span,
    };
    let fallback_arm = MatchArm {
        pattern: Pattern::Wildcard(span),
        guard: None,
        body: else_expr.clone(),
        span,
    };
    Expr::Match(
        Box::new(value.clone()),
        vec![success_arm, fallback_arm],
        span,
    )
}

/// Convert remaining block statements + final expression into a single Expr.
fn stmts_to_body_expr(stmts: &[ast::Stmt], final_expr: Option<&Expr>, span: Span) -> Expr {
    Expr::Block(
        stmts.to_vec(),
        final_expr.map(|e| Box::new(e.clone())),
        span,
    )
}
