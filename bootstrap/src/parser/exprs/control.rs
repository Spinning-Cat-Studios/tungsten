//! Control flow (if/match/lambda) and proof expression parsing.

use crate::ast::*;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use crate::parser::Parser;
impl Parser<'_> {
    // ─────────────────────────────────────────────────────────────────────────
    // Control Flow
    // ─────────────────────────────────────────────────────────────────────────

    pub(in crate::parser) fn parse_if_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::If)?;

        // Check for `if let` syntax (ADR 14.5.26e)
        if self.check(TokenKind::Let) {
            return self.parse_if_let_expr(start);
        }

        let cond = self.parse_expr()?;

        // Check for ML/Haskell-style `then` keyword and give helpful error
        if self.current().kind == TokenKind::Ident && self.current_text() == "then" {
            let span = self.current_span();
            self.errors.push(
                crate::error::ParseError::new(
                    span,
                    crate::error::ParseErrorKind::UnexpectedToken("then".to_string()),
                )
                .with_expected(vec!["`{`".to_string()])
                .with_suggestion(crate::error::Suggestion::new(
                    span,
                    "{",
                    "Tungsten uses braces for if expressions: `if condition { ... } else { ... }`",
                )),
            );
            return None;
        }

        let then_branch = self.parse_block_expr()?;

        let else_branch = if self.eat(TokenKind::Else) {
            if self.check(TokenKind::If) {
                self.parse_if_expr()?
            } else {
                self.parse_block_expr()?
            }
        } else {
            // No else branch - default to unit
            Expr::Unit(Span::empty(self.prev_span().end))
        };

        let end = else_branch.span().end;
        Some(Expr::If(
            Box::new(cond),
            Box::new(then_branch),
            Box::new(else_branch),
            Span::new(start, end),
        ))
    }

    /// Parse `if let P = expr { body }` or `if let P = expr { body } else { fallback }` (ADR 14.5.26e).
    /// Also handles `if let` chains with `&&`-separated conditions (ADR 15.5.26d).
    fn parse_if_let_expr(&mut self, start: u32) -> Option<Expr> {
        self.expect(TokenKind::Let)?;
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::Eq)?;
        // Parse init at precedence above `&&` (prec 2) so `&&` remains as chain separator
        let init = self.parse_expr_prec(3)?;

        // Check for chain continuation with `&&`
        if self.check(TokenKind::AmpAmp) {
            return self.parse_if_let_chain(start, pattern, init);
        }

        let body = self.parse_block_expr()?;

        let else_branch = if self.eat(TokenKind::Else) {
            Some(Box::new(self.parse_block_expr()?))
        } else {
            None
        };

        let end = else_branch
            .as_ref()
            .map_or(body.span().end, |e| e.span().end);
        Some(Expr::IfLet(
            pattern,
            Box::new(init),
            Box::new(body),
            else_branch,
            Span::new(start, end),
        ))
    }

    /// Parse the remainder of an `if let` chain after the first condition (ADR 15.5.26d).
    /// `if let P1 = e1 && let P2 = e2 && guard { body } [else { fallback }]`
    fn parse_if_let_chain(
        &mut self,
        start: u32,
        first_pat: Pattern,
        first_init: Expr,
    ) -> Option<Expr> {
        let mut conditions = vec![IfLetCondition::Bind(first_pat, Box::new(first_init))];

        while self.eat(TokenKind::AmpAmp) {
            if self.check(TokenKind::Let) {
                // `let P = expr` condition
                self.expect(TokenKind::Let)?;
                let pattern = self.parse_pattern()?;
                self.expect(TokenKind::Eq)?;
                // Parse init at precedence above `&&` so next `&&` remains as chain separator
                let init = self.parse_expr_prec(3)?;
                conditions.push(IfLetCondition::Bind(pattern, Box::new(init)));
            } else {
                // Boolean guard condition — also parse above `&&` precedence
                let guard = self.parse_expr_prec(3)?;
                conditions.push(IfLetCondition::Guard(Box::new(guard)));
            }
        }

        let body = self.parse_block_expr()?;

        let else_branch = if self.eat(TokenKind::Else) {
            Some(Box::new(self.parse_block_expr()?))
        } else {
            None
        };

        let end = else_branch
            .as_ref()
            .map_or(body.span().end, |e| e.span().end);
        Some(Expr::IfLetChain(
            conditions,
            Box::new(body),
            else_branch,
            Span::new(start, end),
        ))
    }

    pub(in crate::parser) fn parse_match_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Match)?;

        let scrutinee = self.parse_expr()?;
        self.expect(TokenKind::LBrace)?;

        let mut arms = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            arms.push(self.parse_match_arm()?);

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(TokenKind::RBrace)?.end;
        Some(Expr::Match(
            Box::new(scrutinee),
            arms,
            Span::new(start, end),
        ))
    }

    fn parse_match_arm(&mut self) -> Option<MatchArm> {
        let start = self.current_span().start;
        let pattern = self.parse_pattern()?;

        let guard = if self.eat(TokenKind::If) {
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.expect(TokenKind::FatArrow)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        Some(MatchArm {
            pattern,
            guard,
            body,
            span: Span::new(start, end),
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Lambdas
    // ─────────────────────────────────────────────────────────────────────────

    pub(in crate::parser) fn parse_lambda_fn(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Fn)?;

        let params = self.parse_lambda_params()?;
        self.expect(TokenKind::FatArrow)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        Some(Expr::Lambda(params, Box::new(body), Span::new(start, end)))
    }

    pub(in crate::parser) fn parse_lambda_pipe(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Pipe)?;

        let mut params = Vec::new();
        while !self.check(TokenKind::Pipe) && !self.at_eof() {
            let pat_start = self.current_span().start;
            // Use parse_pattern_atom to avoid ambiguity with | for OR patterns
            let pattern = self.parse_pattern_atom()?;
            let ty = if self.eat(TokenKind::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            let pat_end = ty.as_ref().map_or(pattern.span().end, |t| t.span().end);
            params.push(LambdaParam {
                pattern,
                ty,
                span: Span::new(pat_start, pat_end),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::Pipe)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        Some(Expr::Lambda(params, Box::new(body), Span::new(start, end)))
    }

    fn parse_lambda_params(&mut self) -> Option<Vec<LambdaParam>> {
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(TokenKind::RParen) && !self.at_eof() {
            let start = self.current_span().start;
            let pattern = self.parse_pattern()?;
            let ty = if self.eat(TokenKind::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            let end = ty.as_ref().map_or(pattern.span().end, |t| t.span().end);
            params.push(LambdaParam {
                pattern,
                ty,
                span: Span::new(start, end),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RParen)?;
        Some(params)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Proof Expressions
    // ─────────────────────────────────────────────────────────────────────────

    pub(in crate::parser) fn parse_have_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Have)?;

        let name = self.parse_ident()?;
        self.expect(TokenKind::Colon)?;
        let prop = self.parse_type()?;
        self.expect(TokenKind::Eq)?;
        let proof = self.parse_expr()?;
        self.expect(TokenKind::Semi)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        Some(Expr::Have(
            name,
            prop,
            Box::new(proof),
            Box::new(body),
            Span::new(start, end),
        ))
    }

    pub(in crate::parser) fn parse_show_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Show)?;

        let prop = self.parse_type()?;
        let body = self.parse_block_expr()?;
        let end = body.span().end;

        Some(Expr::Show(prop, Box::new(body), Span::new(start, end)))
    }

    pub(in crate::parser) fn parse_assume_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Assume)?;

        let name = self.parse_ident()?;
        self.expect(TokenKind::Colon)?;
        let prop = self.parse_type()?;
        self.expect(TokenKind::Semi)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        Some(Expr::Assume(
            name,
            prop,
            Box::new(body),
            Span::new(start, end),
        ))
    }
}
