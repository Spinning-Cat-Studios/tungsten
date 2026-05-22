//! `if let` and `if let` chain desugaring (ADR 14.5.26e, ADR 15.5.26d).
//!
//! Desugars `if let P = expr { body }` into:
//!   `match expr { P => body, _ => () }`
//!
//! Desugars `if let P = expr { body } else { fallback }` into:
//!   `match expr { P => body, _ => fallback }`
//!
//! Desugars chains `if let P1 = e1 && let P2 = e2 && guard { body } else { fallback }` into nested match/if:
//!   `match e1 { P1 => match e2 { P2 => if guard { body } else { fallback }, _ => fallback }, _ => fallback }`

use crate::ast::{Expr, IfLetCondition, MatchArm, Pattern};
use crate::span::Span;
use tungsten_core::{Term, Type};

use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

/// Bundled arguments for an `if let` expression.
pub(super) struct IfLetArgs<'e> {
    pub pattern: &'e Pattern,
    pub init: &'e Expr,
    pub body: &'e Expr,
    pub else_branch: Option<&'e Expr>,
}

/// Bundled arguments for an `if let` chain expression (ADR 15.5.26d).
pub(super) struct IfLetChainArgs<'e> {
    pub conditions: &'e [IfLetCondition],
    pub body: &'e Expr,
    pub else_branch: Option<&'e Expr>,
}

impl<'a> Elaborator<'a> {
    /// Elaborate `if let P = expr { body }` or `if let P = expr { body } else { fallback }`.
    pub(super) fn elab_if_let(
        &mut self,
        args: IfLetArgs<'_>,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        self.warn_if_let_irrefutable(args.pattern, span);
        let match_expr =
            build_if_let_match(args.pattern, args.init, args.body, args.else_branch, span);
        if let Some(expected) = expected {
            let term = self.check(&match_expr, expected)?;
            Ok((term, expected.clone()))
        } else {
            self.infer(&match_expr)
        }
    }

    /// Elaborate an `if let` chain (ADR 15.5.26d).
    pub(super) fn elab_if_let_chain(
        &mut self,
        args: IfLetChainArgs<'_>,
        expected: Option<&Type>,
        span: Span,
    ) -> ElabResult<(Term, Type)> {
        let desugared = build_if_let_chain(args.conditions, args.body, args.else_branch, span);
        if let Some(expected) = expected {
            let term = self.check(&desugared, expected)?;
            Ok((term, expected.clone()))
        } else {
            self.infer(&desugared)
        }
    }

    /// Emit W0004 if the pattern is irrefutable (variable or wildcard).
    fn warn_if_let_irrefutable(&mut self, pattern: &Pattern, span: Span) {
        if matches!(pattern, Pattern::Var(_) | Pattern::Wildcard(_)) {
            self.warn(ElabError::new(span, ElabErrorKind::IfLetIrrefutable));
        }
    }
}

/// Build the desugared match expression for a simple `if let`.
fn build_if_let_match(
    pattern: &Pattern,
    init: &Expr,
    body: &Expr,
    else_branch: Option<&Expr>,
    span: Span,
) -> Expr {
    let success_arm = MatchArm {
        pattern: pattern.clone(),
        guard: None,
        body: body.clone(),
        span,
    };
    let fallback_body = match else_branch {
        Some(e) => e.clone(),
        None => Expr::Unit(Span::empty(span.end)),
    };
    let fallback_arm = MatchArm {
        pattern: Pattern::Wildcard(span),
        guard: None,
        body: fallback_body,
        span,
    };
    Expr::Match(
        Box::new(init.clone()),
        vec![success_arm, fallback_arm],
        span,
    )
}

/// Build the desugared expression for an `if let` chain (ADR 15.5.26d).
///
/// Recursively nests conditions:
/// - `IfLetCondition::Bind(pat, init)` → `match init { pat => <rest>, _ => else_branch }`
/// - `IfLetCondition::Guard(expr)` → `if expr { <rest> } else { else_branch }`
///
/// When all conditions are consumed, the innermost expression is `body`.
fn build_if_let_chain(
    conditions: &[IfLetCondition],
    body: &Expr,
    else_branch: Option<&Expr>,
    span: Span,
) -> Expr {
    let fallback = match else_branch {
        Some(e) => e.clone(),
        None => Expr::Unit(Span::empty(span.end)),
    };
    build_chain_inner(conditions, body, &fallback, span)
}

fn build_chain_inner(
    conditions: &[IfLetCondition],
    body: &Expr,
    fallback: &Expr,
    span: Span,
) -> Expr {
    match conditions.split_first() {
        None => body.clone(),
        Some((cond, rest)) => {
            let inner = build_chain_inner(rest, body, fallback, span);
            match cond {
                IfLetCondition::Bind(pattern, init) => {
                    let success_arm = MatchArm {
                        pattern: pattern.clone(),
                        guard: None,
                        body: inner,
                        span,
                    };
                    let fallback_arm = MatchArm {
                        pattern: Pattern::Wildcard(span),
                        guard: None,
                        body: fallback.clone(),
                        span,
                    };
                    Expr::Match(
                        Box::new(*init.clone()),
                        vec![success_arm, fallback_arm],
                        span,
                    )
                }
                IfLetCondition::Guard(guard) => Expr::If(
                    Box::new(*guard.clone()),
                    Box::new(inner),
                    Box::new(fallback.clone()),
                    span,
                ),
            }
        }
    }
}
