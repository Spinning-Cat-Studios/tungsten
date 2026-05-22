//! Parsing patterns for match expressions and destructuring.

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use super::Parser;

impl Parser<'_> {
    pub(super) fn parse_pattern(&mut self) -> Option<Pattern> {
        self.parse_pattern_or()
    }

    fn parse_pattern_or(&mut self) -> Option<Pattern> {
        let mut left = self.parse_pattern_atom()?;

        while self.check(TokenKind::Pipe) {
            self.advance();
            let right = self.parse_pattern_atom()?;
            let span = Span::new(left.span().start, right.span().end);
            left = Pattern::Or(Box::new(left), Box::new(right), span);
        }

        Some(left)
    }

    pub(super) fn parse_pattern_atom(&mut self) -> Option<Pattern> {
        let start = self.current_span().start;

        match self.current().kind {
            TokenKind::Underscore => {
                self.advance();
                Some(Pattern::Wildcard(Span::new(start, self.prev_span().end)))
            }
            TokenKind::IntLiteral => {
                let text = self.current_text();
                let value = self.parse_int_literal(text);
                self.advance();
                Some(Pattern::Literal(LiteralPattern::Int(
                    value,
                    Span::new(start, self.prev_span().end),
                )))
            }
            TokenKind::True | TokenKind::False => {
                let value = self.current().kind == TokenKind::True;
                self.advance();
                Some(Pattern::Literal(LiteralPattern::Bool(
                    value,
                    Span::new(start, self.prev_span().end),
                )))
            }
            TokenKind::StringLiteral => {
                let text = self.current_text();
                let value = self.unescape_string(&text[1..text.len() - 1]);
                self.advance();
                Some(Pattern::Literal(LiteralPattern::String(
                    value,
                    Span::new(start, self.prev_span().end),
                )))
            }
            TokenKind::LParen => self.parse_paren_or_tuple_pattern(start),
            TokenKind::Ident => self.parse_ident_pattern(start),
            _ => {
                self.error(ParseErrorKind::InvalidPattern);
                Some(Pattern::Error(self.current_span()))
            }
        }
    }

    /// Parse a parenthesized pattern `(P)`, empty tuple `()`, or tuple `(A, B)`.
    fn parse_paren_or_tuple_pattern(&mut self, start: u32) -> Option<Pattern> {
        self.advance();
        if self.check(TokenKind::RParen) {
            self.advance();
            return Some(Pattern::Tuple(
                Vec::new(),
                Span::new(start, self.prev_span().end),
            ));
        }

        let first = self.parse_pattern()?;
        if self.check(TokenKind::Comma) {
            let mut patterns = vec![first];
            while self.eat(TokenKind::Comma) {
                if self.check(TokenKind::RParen) {
                    break;
                }
                patterns.push(self.parse_pattern()?);
            }
            let end = self.expect(TokenKind::RParen)?.end;
            Some(Pattern::Tuple(patterns, Span::new(start, end)))
        } else {
            self.expect(TokenKind::RParen)?;
            Some(first)
        }
    }

    /// Parse an identifier-led pattern: variable, constructor, or qualified path.
    fn parse_ident_pattern(&mut self, start: u32) -> Option<Pattern> {
        let path = self.parse_path()?;
        if self.check(TokenKind::LParen) {
            // Constructor pattern with arguments
            self.advance();
            let mut args = Vec::new();
            while !self.check(TokenKind::RParen) && !self.at_eof() {
                args.push(self.parse_pattern()?);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            let end = self.expect(TokenKind::RParen)?.end;
            Some(Pattern::Constructor(path, args, Span::new(start, end)))
        } else if path.is_simple() {
            Some(Pattern::Var(path.item_name().clone()))
        } else {
            Some(Pattern::Constructor(path.clone(), Vec::new(), path.span))
        }
    }
}
