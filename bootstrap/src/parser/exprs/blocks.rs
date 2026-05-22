//! Block, record literal, let statement/expression, and error recovery parsing.

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use crate::parser::Parser;
impl Parser<'_> {
    pub(in crate::parser) fn parse_block_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;
        self.expect(TokenKind::LBrace)?;

        let mut stmts = Vec::new();
        let mut final_expr = None;

        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            // Check if this is the final expression (no semicolon)
            let maybe_stmt = match self.current().kind {
                TokenKind::Let => {
                    if let Some(stmt) = self.parse_let_stmt() {
                        stmts.push(stmt);
                    } else {
                        self.synchronize_to_stmt();
                    }
                    continue;
                }
                // `fn foo` is a nested function definition (item)
                // `fn(` is a lambda expression
                TokenKind::Fn if self.check_ahead(TokenKind::Ident) => {
                    if let Some(item) = self.parse_item() {
                        stmts.push(Stmt::Item(item));
                    } else {
                        self.synchronize_to_stmt();
                    }
                    continue;
                }
                TokenKind::Type
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Theorem
                | TokenKind::Lemma
                | TokenKind::Axiom => {
                    if let Some(item) = self.parse_item() {
                        stmts.push(Stmt::Item(item));
                    } else {
                        self.synchronize_to_stmt();
                    }
                    continue;
                }
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
    pub(in crate::parser) fn is_record_literal_start(&self) -> bool {
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
    pub(in crate::parser) fn parse_record_literal(&mut self) -> Option<Expr> {
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

    /// Parse a named record constructor: `TypeName { field: value, ... }` or
    /// `TypeName { ...spread, field: value, ... }` (ADR 13.5.26h/i)
    ///
    /// Called from `parse_postfix` after the path has already been parsed and
    /// disambiguated via the uppercase-initial convention.
    pub(in crate::parser) fn parse_named_record_fields(
        &mut self,
        name: crate::ast::Path,
    ) -> Option<Expr> {
        let start = name.span.start;
        self.expect(TokenKind::LBrace)?;

        // Check for optional spread at the start
        let spread = if self.eat(TokenKind::DotDotDot) {
            let spread_expr = self.parse_postfix()?;
            if !self.check(TokenKind::RBrace) {
                self.expect(TokenKind::Comma)?;
            }
            Some(Box::new(spread_expr))
        } else {
            None
        };

        let mut fields = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            // Reject second spread
            if self.check(TokenKind::DotDotDot) {
                self.error(ParseErrorKind::InvalidExpression);
                return None;
            }

            let field_name = self.parse_ident()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((field_name, value));

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(TokenKind::RBrace)?.end;
        Some(Expr::NamedRecord {
            name,
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

        // Check for `else` clause (let-else)
        if self.eat(TokenKind::Else) {
            let else_expr = self.parse_expr()?;
            let end = if self.eat(TokenKind::Semi) {
                self.prev_span().end
            } else {
                else_expr.span().end
            };
            return Some(Stmt::LetElse(
                pattern,
                ty,
                value,
                else_expr,
                Span::new(start, end),
            ));
        }

        // Expect semicolon
        let end = if self.eat(TokenKind::Semi) {
            self.prev_span().end
        } else {
            value.span().end
        };

        Some(Stmt::Let(pattern, ty, value, Span::new(start, end)))
    }

    pub(in crate::parser) fn parse_let_expr(&mut self) -> Option<Expr> {
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

        // Check for `else` clause (let-else)
        if self.eat(TokenKind::Else) {
            let else_expr = self.parse_expr()?;
            self.expect(TokenKind::Semi)?;
            let body = self.parse_expr()?;
            let end = body.span().end;
            return Some(Expr::LetElse(
                pattern,
                ty,
                Box::new(value),
                Box::new(else_expr),
                Box::new(body),
                Span::new(start, end),
            ));
        }

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
}
