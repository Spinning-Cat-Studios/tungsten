//! Parsing expressions using Pratt parsing for operators.

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use super::Parser;

impl Parser<'_> {
    // ─────────────────────────────────────────────────────────────────────────
    // Expressions (Pratt Parser)
    // ─────────────────────────────────────────────────────────────────────────

    pub(super) fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_expr_prec(0)
    }

    fn parse_expr_prec(&mut self, min_prec: u8) -> Option<Expr> {
        let mut left = self.parse_unary()?;

        loop {
            let Some((op, prec)) = self.peek_binop() else {
                break;
            };

            if prec < min_prec {
                break;
            }

            self.advance(); // consume operator

            let next_prec = if op.is_right_assoc() { prec } else { prec + 1 };
            let right = self.parse_expr_prec(next_prec)?;

            let span = Span::new(left.span().start, right.span().end);
            left = Expr::Binary(Box::new(left), op, Box::new(right), span);
        }

        // Type annotation: `e : T`
        if self.check(TokenKind::Colon) && !self.check_ahead(TokenKind::Colon) {
            self.advance();
            let ty = self.parse_type()?;
            let span = Span::new(left.span().start, ty.span().end);
            left = Expr::Annot(Box::new(left), ty, span);
        }

        Some(left)
    }

    fn peek_binop(&self) -> Option<(BinOp, u8)> {
        let op = match self.current().kind {
            TokenKind::PipePipe => BinOp::Or,
            TokenKind::AmpAmp => BinOp::And,
            TokenKind::EqEq => BinOp::Eq,
            TokenKind::Ne => BinOp::Ne,
            TokenKind::Lt => BinOp::Lt,
            TokenKind::Le => BinOp::Le,
            TokenKind::Gt => BinOp::Gt,
            TokenKind::Ge => BinOp::Ge,
            TokenKind::PipeRight => BinOp::Pipe,
            TokenKind::Plus => BinOp::Add,
            TokenKind::PlusPlus => BinOp::Concat,
            TokenKind::Minus => BinOp::Sub,
            TokenKind::Star => BinOp::Mul,
            TokenKind::Slash => BinOp::Div,
            TokenKind::Percent => BinOp::Mod,
            _ => return None,
        };
        Some((op, op.precedence()))
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let start = self.current_span().start;

        match self.current().kind {
            TokenKind::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                let span = Span::new(start, expr.span().end);
                Some(Expr::Unary(UnaryOp::Not, Box::new(expr), span))
            }
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                let span = Span::new(start, expr.span().end);
                Some(Expr::Unary(UnaryOp::Neg, Box::new(expr), span))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_atom()?;

        loop {
            match self.current().kind {
                // Function call: `f(args)`
                TokenKind::LParen => {
                    let args = self.parse_call_args()?;
                    let span = Span::new(expr.span().start, self.prev_span().end);
                    expr = Expr::App(Box::new(expr), args, span);
                }
                // Field access: `e.field`
                TokenKind::Dot => {
                    self.advance();
                    let field = self.parse_ident()?;
                    let span = Span::new(expr.span().start, field.span.end);
                    expr = Expr::Field(Box::new(expr), field, span);
                }
                // Type application: `e::<T>`
                TokenKind::ColonColon if self.check_ahead(TokenKind::Lt) => {
                    self.advance();
                    let args = self.parse_type_args()?;
                    let span = Span::new(expr.span().start, self.prev_span().end);
                    expr = Expr::TypeApp(Box::new(expr), args, span);
                }
                _ => break,
            }
        }

        Some(expr)
    }

    fn parse_atom(&mut self) -> Option<Expr> {
        let start = self.current_span().start;

        match self.current().kind {
            // Literals
            TokenKind::IntLiteral => {
                let text = self.current_text();
                let value = self.parse_int_literal(&text);
                self.advance();
                Some(Expr::IntLiteral(
                    value,
                    Span::new(start, self.prev_span().end),
                ))
            }
            TokenKind::True => {
                self.advance();
                Some(Expr::BoolLiteral(
                    true,
                    Span::new(start, self.prev_span().end),
                ))
            }
            TokenKind::False => {
                self.advance();
                Some(Expr::BoolLiteral(
                    false,
                    Span::new(start, self.prev_span().end),
                ))
            }
            TokenKind::StringLiteral => {
                let text = self.current_text();
                // Strip quotes and unescape
                let value = self.unescape_string(&text[1..text.len() - 1]);
                self.advance();
                Some(Expr::StringLiteral(
                    value,
                    Span::new(start, self.prev_span().end),
                ))
            }

            // Identifiers and paths
            TokenKind::Ident => {
                let path = self.parse_path()?;
                Some(Expr::Path(path))
            }

            // Parentheses, tuples, unit
            TokenKind::LParen => {
                self.advance();
                if self.check(TokenKind::RParen) {
                    self.advance();
                    Some(Expr::Unit(Span::new(start, self.prev_span().end)))
                } else {
                    let first = self.parse_expr()?;
                    if self.check(TokenKind::Comma) {
                        // Tuple
                        let mut elements = vec![first];
                        while self.eat(TokenKind::Comma) {
                            if self.check(TokenKind::RParen) {
                                break;
                            }
                            elements.push(self.parse_expr()?);
                        }
                        let end = self.expect(TokenKind::RParen)?.end;
                        Some(Expr::Tuple(elements, Span::new(start, end)))
                    } else {
                        let end = self.expect(TokenKind::RParen)?.end;
                        Some(Expr::Paren(Box::new(first), Span::new(start, end)))
                    }
                }
            }

            // Block or record literal
            TokenKind::LBrace => {
                // Disambiguate: { ident: ... } is record literal
                if self.is_record_literal_start() {
                    self.parse_record_literal()
                } else {
                    self.parse_block_expr()
                }
            }

            // If expression
            TokenKind::If => self.parse_if_expr(),

            // Match expression
            TokenKind::Match => self.parse_match_expr(),

            // Let expression
            TokenKind::Let => self.parse_let_expr(),

            // Lambda: `fn(x: T) => body`
            TokenKind::Fn => self.parse_lambda_fn(),

            // Return
            TokenKind::Return => {
                self.advance();
                let value = if self.can_start_expr() {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                let end = value
                    .as_ref()
                    .map_or(self.prev_span().end, |e| e.span().end);
                Some(Expr::Return(value, Span::new(start, end)))
            }

            // Proof constructs
            TokenKind::Have => self.parse_have_expr(),
            TokenKind::Show => self.parse_show_expr(),
            TokenKind::Assume => self.parse_assume_expr(),
            TokenKind::Sorry => {
                self.advance();
                Some(Expr::Sorry(Sorry::new(Span::new(
                    start,
                    self.prev_span().end,
                ))))
            }
            TokenKind::Refl => {
                self.advance();
                Some(Expr::Refl(Span::new(start, self.prev_span().end)))
            }

            // Pipe closure: `|x| body`
            TokenKind::Pipe => self.parse_lambda_pipe(),

            _ => {
                // Check if this is a reserved keyword being used as an identifier
                // The lexer already emits an error for reserved keywords, so we
                // just need to advance past it and emit a generic error if it's
                // not a reserved keyword.
                if !self.current().kind.is_reserved() {
                    self.error(ParseErrorKind::InvalidExpression);
                }
                let span = self.current_span();
                self.advance(); // Always advance to avoid infinite loops
                Some(Expr::Error(span))
            }
        }
    }

    fn parse_call_args(&mut self) -> Option<Vec<Expr>> {
        self.expect(TokenKind::LParen)?;

        let mut args = Vec::new();
        while !self.check(TokenKind::RParen) && !self.at_eof() {
            args.push(self.parse_expr()?);

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RParen)?;
        Some(args)
    }

    pub(super) fn parse_block_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::LBrace)?;

        let mut stmts = Vec::new();
        let mut final_expr = None;

        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            // Check if this is the final expression (no semicolon)
            let maybe_stmt = match self.current().kind {
                TokenKind::Let => {
                    match self.parse_let_stmt() {
                        Some(stmt) => {
                            stmts.push(stmt);
                            continue;
                        }
                        None => {
                            // Recovery: skip to next statement boundary
                            self.synchronize_to_stmt();
                            continue;
                        }
                    }
                }
                // `fn foo` is a nested function definition (item)
                // `fn(` is a lambda expression
                TokenKind::Fn if self.check_ahead(TokenKind::Ident) => match self.parse_item() {
                    Some(item) => {
                        stmts.push(Stmt::Item(item));
                        continue;
                    }
                    None => {
                        self.synchronize_to_stmt();
                        continue;
                    }
                },
                TokenKind::Type
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Theorem
                | TokenKind::Lemma
                | TokenKind::Axiom => match self.parse_item() {
                    Some(item) => {
                        stmts.push(Stmt::Item(item));
                        continue;
                    }
                    None => {
                        self.synchronize_to_stmt();
                        continue;
                    }
                },
                _ => self.parse_expr(),
            };

            let Some(expr) = maybe_stmt else {
                // Failed to parse expression - recover to next statement
                self.synchronize_to_stmt();
                continue;
            };

            if self.check(TokenKind::Semi) {
                self.advance();
                let stmt_span = Span::new(expr.span().start, self.prev_span().end);
                stmts.push(Stmt::Expr(expr, stmt_span));
            } else if self.check(TokenKind::RBrace) {
                final_expr = Some(Box::new(expr));
            } else {
                // Implicit semicolon after block-like expressions
                let stmt_span = expr.span();
                stmts.push(Stmt::Expr(expr, stmt_span));
            }
        }

        let end = self.expect(TokenKind::RBrace)?.end;
        Some(Expr::Block(stmts, final_expr, Span::new(start, end)))
    }

    /// Synchronize to the next statement boundary for error recovery.
    ///
    /// This helps reduce cascading errors by skipping to a known recovery point.
    fn synchronize_to_stmt(&mut self) {
        while !self.at_eof() {
            // Stop at closing brace (end of block)
            if self.check(TokenKind::RBrace) {
                return;
            }

            // Stop after semicolon
            if self.current().kind == TokenKind::Semi {
                self.advance();
                return;
            }

            // Stop at statement-starting tokens
            match self.current().kind {
                TokenKind::Let
                | TokenKind::If
                | TokenKind::Match
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Return
                | TokenKind::Fn
                | TokenKind::Type
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Theorem
                | TokenKind::Lemma
                | TokenKind::Axiom => {
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Check if the next tokens form a record literal start: `{ ident: ...` or `{ ...expr`
    fn is_record_literal_start(&self) -> bool {
        // { IDENT : ... } is a record literal
        // { ... expr } is a record literal with spread
        // { let ... } or { expr ; ... } is a block
        if !self.check(TokenKind::LBrace) {
            return false;
        }

        // Check for { ... (spread syntax)
        if self.check_ahead_n(1, |k| k == TokenKind::DotDotDot) {
            return true;
        }

        // Check for { IDENT : pattern
        self.check_ahead_n(1, |k| k == TokenKind::Ident)
            && self.check_ahead_n(2, |k| k == TokenKind::Colon)
    }

    /// Parse a record literal: `{ field1: expr1, field2: expr2, ... }`
    /// or with spread: `{ ...base, field1: expr1, ... }`
    fn parse_record_literal(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::LBrace)?;

        // Check for optional spread at the start
        let spread = if self.eat(TokenKind::DotDotDot) {
            let spread_expr = self.parse_postfix()?;
            // After spread, we expect either a comma (for more fields) or closing brace
            if !self.check(TokenKind::RBrace) {
                self.expect(TokenKind::Comma)?;
            }
            Some(Box::new(spread_expr))
        } else {
            None
        };

        let mut fields = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            // Check for second spread (error case)
            if self.check(TokenKind::DotDotDot) {
                self.error(ParseErrorKind::InvalidExpression);
                return None;
            }

            let name = self.parse_ident()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((name, value));

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(TokenKind::RBrace)?.end;
        Some(Expr::RecordLit {
            spread,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_let_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;
        self.expect(TokenKind::Let)?;

        let pattern = self.parse_pattern()?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;

        // Expect semicolon
        let end = if self.eat(TokenKind::Semi) {
            self.prev_span().end
        } else {
            value.span().end
        };

        Some(Stmt::Let(pattern, ty, value, Span::new(start, end)))
    }

    fn parse_let_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Let)?;

        let pattern = self.parse_pattern()?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;

        // Expect semicolon then continuation
        self.expect(TokenKind::Semi)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        Some(Expr::Let(
            pattern,
            ty,
            Box::new(value),
            Box::new(body),
            Span::new(start, end),
        ))
    }

    fn parse_if_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::If)?;

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

    fn parse_match_expr(&mut self) -> Option<Expr> {
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

    fn parse_lambda_fn(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Fn)?;

        let params = self.parse_lambda_params()?;
        self.expect(TokenKind::FatArrow)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        Some(Expr::Lambda(params, Box::new(body), Span::new(start, end)))
    }

    fn parse_lambda_pipe(&mut self) -> Option<Expr> {
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

    fn parse_have_expr(&mut self) -> Option<Expr> {
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

    fn parse_show_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::Show)?;

        let prop = self.parse_type()?;
        let body = self.parse_block_expr()?;
        let end = body.span().end;

        Some(Expr::Show(prop, Box::new(body), Span::new(start, end)))
    }

    fn parse_assume_expr(&mut self) -> Option<Expr> {
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
