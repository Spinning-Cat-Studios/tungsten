//! Literal and simple atom expression parsers.
//!
//! Handles integer, boolean, string literals as well as
//! sorry, refl, error/reserved, and brace expressions.

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use crate::parser::Parser;
impl Parser<'_> {
    pub(in crate::parser) fn parse_int_literal_expr(&mut self, start: u32) -> Option<Expr> {
        let text = self.current_text();
        let value = self.parse_int_literal(text);
        self.advance();
        Some(Expr::IntLiteral(
            value,
            Span::new(start, self.prev_span().end),
        ))
    }

    pub(in crate::parser) fn parse_bool_literal_expr(&mut self, start: u32) -> Option<Expr> {
        let value = self.current().kind == TokenKind::True;
        self.advance();
        Some(Expr::BoolLiteral(
            value,
            Span::new(start, self.prev_span().end),
        ))
    }

    pub(in crate::parser) fn parse_string_literal_expr(&mut self, start: u32) -> Option<Expr> {
        let text = self.current_text();
        let value = self.unescape_string(&text[1..text.len() - 1]);
        self.advance();
        Some(Expr::StringLiteral(
            value,
            Span::new(start, self.prev_span().end),
        ))
    }

    pub(in crate::parser) fn parse_error_or_reserved(&mut self) -> Option<Expr> {
        if !self.current().kind.is_reserved() {
            self.error(ParseErrorKind::InvalidExpression);
        }
        let span = self.current_span();
        self.advance();
        Some(Expr::Error(span))
    }

    pub(in crate::parser) fn parse_brace_expr(&mut self) -> Option<Expr> {
        if self.is_record_literal_start() {
            self.parse_record_literal()
        } else {
            self.parse_block_expr()
        }
    }

    pub(in crate::parser) fn parse_sorry_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance();
        Some(Expr::Sorry(Sorry::new(Span::new(
            start,
            self.prev_span().end,
        ))))
    }

    pub(in crate::parser) fn parse_refl_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance();
        Some(Expr::Refl(Span::new(start, self.prev_span().end)))
    }

    pub(in crate::parser) fn parse_subst_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance(); // consume `subst`
        self.expect(TokenKind::LParen)?;
        let proof = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let motive = self.parse_motive()?;
        self.expect(TokenKind::Comma)?;
        let witness = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let span = Span::new(start, self.prev_span().end);
        Some(Expr::Subst(
            Box::new(proof),
            motive,
            Box::new(witness),
            span,
        ))
    }

    /// Parse a motive in `subst(proof, motive, witness)` (ADR 21.5.26g).
    ///
    /// If the next token is `|`, parse as a motive lambda: `|x: τ| <type-expr>`.
    /// Otherwise, parse as a raw expression (will be rejected by the elaborator).
    fn parse_motive(&mut self) -> Option<Motive> {
        if self.check(TokenKind::Pipe) {
            let motive_start = self.current_span().start;
            self.advance(); // consume `|`
            let param_start = self.current_span().start;
            let pattern = self.parse_pattern_atom()?;
            self.expect(TokenKind::Colon)?;
            let param_ty = self.parse_type()?;
            let param_end = param_ty.span().end;
            let param = LambdaParam {
                pattern,
                ty: Some(param_ty),
                span: Span::new(param_start, param_end),
            };
            self.expect(TokenKind::Pipe)?;
            let body = self.parse_type()?;
            let span = Span::new(motive_start, body.span().end);
            Some(Motive::Lambda(param, Box::new(body), span))
        } else {
            let expr = self.parse_expr()?;
            Some(Motive::Expr(Box::new(expr)))
        }
    }

    pub(in crate::parser) fn parse_sym_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance(); // consume `sym`
        self.expect(TokenKind::LParen)?;
        let proof = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let span = Span::new(start, self.prev_span().end);
        Some(Expr::Sym(Box::new(proof), span))
    }

    pub(in crate::parser) fn parse_trans_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance(); // consume `trans`
        self.expect(TokenKind::LParen)?;
        let h1 = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let h2 = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let span = Span::new(start, self.prev_span().end);
        Some(Expr::Trans(Box::new(h1), Box::new(h2), span))
    }

    pub(in crate::parser) fn parse_cong_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance(); // consume `cong`
        self.expect(TokenKind::LParen)?;
        let f = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let proof = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let span = Span::new(start, self.prev_span().end);
        Some(Expr::Cong(Box::new(f), Box::new(proof), span))
    }

    pub(in crate::parser) fn parse_natind_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance(); // consume `natind`
        self.expect(TokenKind::LParen)?;
        let motive = self.parse_motive()?;
        self.expect(TokenKind::Comma)?;
        let base = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let step = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let n = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let span = Span::new(start, self.prev_span().end);
        Some(Expr::NatInd(
            motive,
            Box::new(base),
            Box::new(step),
            Box::new(n),
            span,
        ))
    }

    pub(in crate::parser) fn parse_natrec_expr(&mut self, start: u32) -> Option<Expr> {
        self.advance(); // consume `natrec`
        self.expect(TokenKind::LParen)?;
        let result_ty = self.parse_type()?;
        self.expect(TokenKind::Comma)?;
        let base = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let step = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let n = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let span = Span::new(start, self.prev_span().end);
        Some(Expr::NatRec(
            Box::new(result_ty),
            Box::new(base),
            Box::new(step),
            Box::new(n),
            span,
        ))
    }
}
