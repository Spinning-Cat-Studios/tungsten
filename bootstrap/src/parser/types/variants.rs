//! Parsing type definition constructs: variants, struct fields, enum variants.

use crate::ast::*;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use crate::parser::Parser;
impl Parser<'_> {
    pub(in crate::parser) fn parse_variants(&mut self) -> Option<Vec<Variant>> {
        let mut variants = Vec::new();

        // Skip leading pipe if present
        self.eat(TokenKind::Pipe);

        loop {
            if !self.check_ident() && !self.check(TokenKind::Pub) {
                break;
            }

            let variant = self.parse_variant()?;
            variants.push(variant);

            if !self.eat(TokenKind::Pipe) {
                break;
            }
        }

        Some(variants)
    }

    fn parse_variant(&mut self) -> Option<Variant> {
        let start = self.current_span().start;

        // Parse optional visibility modifier on the constructor
        let visibility = if self.check(TokenKind::Pub) {
            Some(self.parse_visibility())
        } else {
            None
        };

        let name = self.parse_ident()?;

        let fields = if self.check(TokenKind::LParen) {
            self.advance();
            let mut fields = Vec::new();
            while !self.check(TokenKind::RParen) && !self.at_eof() {
                fields.push(self.parse_variant_field()?);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            fields
        } else {
            Vec::new()
        };

        let end = if fields.is_empty() {
            name.span.end
        } else {
            self.prev_span().end
        };

        Some(Variant {
            visibility,
            name,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_variant_field(&mut self) -> Option<Field> {
        let start = self.current_span().start;

        // Check for named field: `name: Type`
        if self.check_ident() && self.check_ahead(TokenKind::Colon) {
            let name = self.parse_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let end = ty.span().end;
            Some(Field {
                name: Some(name),
                ty,
                span: Span::new(start, end),
            })
        } else {
            // Positional field: just a type
            let ty = self.parse_type()?;
            let end = ty.span().end;
            Some(Field {
                name: None,
                ty,
                span: Span::new(start, end),
            })
        }
    }

    pub(in crate::parser) fn parse_struct_fields(&mut self) -> Option<Vec<Field>> {
        let mut fields = Vec::new();

        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            let start = self.current_span().start;
            let name = self.parse_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let end = ty.span().end;

            fields.push(Field {
                name: Some(name),
                ty,
                span: Span::new(start, end),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        Some(fields)
    }

    pub(in crate::parser) fn parse_enum_variants(&mut self) -> Option<Vec<Variant>> {
        let mut variants = Vec::new();

        while !self.check(TokenKind::RBrace) && !self.at_eof() {
            let variant = self.parse_variant()?;
            variants.push(variant);

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        Some(variants)
    }
}
