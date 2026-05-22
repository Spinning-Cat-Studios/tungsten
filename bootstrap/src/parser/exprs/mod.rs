//! Parsing expressions using Pratt parsing for operators.
//!
//! Core expression parsing: Pratt parser, unary/postfix operators, atoms.
//! Block/record/let parsing is in `blocks.rs`.
//! Control flow and proof expressions are in `control.rs`.

mod blocks;
mod control;
mod literals;

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

    pub(super) fn parse_expr_prec(&mut self, min_prec: u8) -> Option<Expr> {
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

    pub(super) fn parse_postfix(&mut self) -> Option<Expr> {
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
                // Try operator: `e?`
                TokenKind::Question => {
                    self.advance();
                    let span = Span::new(expr.span().start, self.prev_span().end);
                    expr = Expr::Try(Box::new(expr), span);
                }
                _ => break,
            }
        }

        // Named record constructor: `TypeName { field: value, ... }` (ADR 13.5.26h)
        // Disambiguate from block expressions by requiring the path's last segment
        // to be CamelCase (starts with uppercase AND contains a lowercase letter).
        // This rejects SCREAMING_SNAKE_CASE constants like INVALID_HANDLE.
        // Edge case: all-uppercase type names like `IO` or `DB` would NOT be
        // recognized as named records. No such types exist in the codebase today;
        // if they appear later, this heuristic will need revisiting.
        if self.check(TokenKind::LBrace) {
            if let Expr::Path(ref path) = expr {
                if let Some(last) = path.segments.last() {
                    if last.name.starts_with(|c: char| c.is_ascii_uppercase())
                        && last.name.chars().any(|c| c.is_ascii_lowercase())
                    {
                        let path = path.clone();
                        return self.parse_named_record_fields(path);
                    }
                }
            }
        }

        Some(expr)
    }

    fn parse_atom(&mut self) -> Option<Expr> {
        let start = self.current_span().start;

        match self.current().kind {
            // Literals
            TokenKind::IntLiteral => self.parse_int_literal_expr(start),
            TokenKind::True | TokenKind::False => self.parse_bool_literal_expr(start),
            TokenKind::StringLiteral => self.parse_string_literal_expr(start),

            // Identifiers and paths
            TokenKind::Ident => {
                let path = self.parse_path()?;
                Some(Expr::Path(path))
            }

            // Parentheses, tuples, unit
            TokenKind::LParen => self.parse_paren_or_tuple_expr(start),

            // Block or record literal
            TokenKind::LBrace => self.parse_brace_expr(),

            // Compound expressions
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Let => self.parse_let_expr(),
            TokenKind::Fn => self.parse_lambda_fn(),
            TokenKind::Return => self.parse_return_expr(start),
            TokenKind::TryBlock => self.parse_try_block_expr(start),

            // Proof constructs
            TokenKind::Have => self.parse_have_expr(),
            TokenKind::Show => self.parse_show_expr(),
            TokenKind::Assume => self.parse_assume_expr(),
            TokenKind::Sorry => self.parse_sorry_expr(start),
            TokenKind::Refl => self.parse_refl_expr(start),
            TokenKind::Subst => self.parse_subst_expr(start),
            TokenKind::Sym => self.parse_sym_expr(start),
            TokenKind::Trans => self.parse_trans_expr(start),
            TokenKind::Cong => self.parse_cong_expr(start),
            TokenKind::NatInd => self.parse_natind_expr(start),
            TokenKind::NatRec => self.parse_natrec_expr(start),

            // Pipe closure: `|x| body`
            TokenKind::Pipe => self.parse_lambda_pipe(),

            _ => self.parse_error_or_reserved(),
        }
    }

    pub(super) fn parse_call_args(&mut self) -> Option<Vec<Expr>> {
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

    /// Parse a `return` expression with optional value.
    fn parse_return_expr(&mut self, start: u32) -> Option<Expr> {
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

    /// Parse a `try { body }` block expression (ADR 15.5.26d).
    fn parse_try_block_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance(); // consume `try`
                        // parse_block_expr handles { stmts; final_expr }
        let block = self.parse_block_expr()?;
        let end = block.span().end;
        Some(Expr::TryBlock(Box::new(block), Span::new(start, end)))
    }

    /// Parse a parenthesized expression `(E)`, unit `()`, or tuple `(A, B, C)`.
    fn parse_paren_or_tuple_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance();
        if self.check(TokenKind::RParen) {
            self.advance();
            return Some(Expr::Unit(Span::new(start, self.prev_span().end)));
        }

        let first = self.parse_expr()?;
        if self.check(TokenKind::Comma) {
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
