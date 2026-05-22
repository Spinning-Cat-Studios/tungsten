//! Token navigation utilities for the parser.
//!
//! Core cursor operations: peek, advance, eat, expect.
//! Error reporting and extended lookahead are in `token_support.rs`.

use crate::error::ParseErrorKind;
use crate::span::Span;
use crate::token::TokenKind;

use super::Parser;

impl<'a> Parser<'a> {
    pub(crate) fn current(&self) -> &crate::token::Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof_token)
    }

    pub(crate) fn current_span(&self) -> Span {
        self.current().span
    }

    pub(crate) fn current_text(&self) -> &str {
        self.current().text(self.source)
    }

    pub(crate) fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            Span::empty(0)
        }
    }

    pub(crate) fn at_eof(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    pub(crate) fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    pub(crate) fn check_ident(&self) -> bool {
        self.current().kind == TokenKind::Ident
    }

    pub(crate) fn check_ahead(&self, kind: TokenKind) -> bool {
        self.tokens
            .get(self.pos + 1)
            .map_or(false, |t| t.kind == kind)
    }

    pub(crate) fn can_start_expr(&self) -> bool {
        self.current().kind.can_start_expr()
    }

    pub(crate) fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
    }

    pub(crate) fn eat(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, kind: TokenKind) -> Option<Span> {
        if self.check(kind) {
            let span = self.current_span();
            self.advance();
            Some(span)
        } else {
            self.error_expected(kind);
            None
        }
    }
}
