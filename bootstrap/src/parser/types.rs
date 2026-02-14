//! Parsing type expressions and type definitions (variants, structs, enums).

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

use super::Parser;

impl Parser<'_> {
    // ─────────────────────────────────────────────────────────────────────────
    // Type Expressions
    // ─────────────────────────────────────────────────────────────────────────

    pub(super) fn parse_type(&mut self) -> Option<TypeExpr> {
        self.parse_type_prec(0)
    }

    fn parse_type_prec(&mut self, min_prec: u8) -> Option<TypeExpr> {
        let mut left = self.parse_type_atom()?;

        loop {
            // Arrow type: A -> B (right associative, precedence 1)
            if min_prec <= 1 && self.check(TokenKind::Arrow) {
                self.advance();
                let right = self.parse_type_prec(1)?;
                let span = Span::new(left.span().start, right.span().end);
                left = TypeExpr::Arrow(Box::new(left), Box::new(right), span);
                continue;
            }

            // Product type: A * B (precedence 3)
            if min_prec <= 3 && self.check(TokenKind::Star) {
                self.advance();
                let right = self.parse_type_prec(4)?;
                let span = Span::new(left.span().start, right.span().end);
                left = TypeExpr::Product(Box::new(left), Box::new(right), span);
                continue;
            }

            // Sum type: A + B (precedence 2)
            if min_prec <= 2 && self.check(TokenKind::Plus) {
                self.advance();
                let right = self.parse_type_prec(3)?;
                let span = Span::new(left.span().start, right.span().end);
                left = TypeExpr::Sum(Box::new(left), Box::new(right), span);
                continue;
            }

            break;
        }

        Some(left)
    }

    fn parse_type_atom(&mut self) -> Option<TypeExpr> {
        let start = self.current_span().start;

        match self.current().kind {
            TokenKind::Bool => {
                self.advance();
                Some(TypeExpr::Path(Path::simple(Ident::new(
                    "Bool",
                    Span::new(start, self.prev_span().end),
                ))))
            }
            TokenKind::Nat => {
                self.advance();
                Some(TypeExpr::Path(Path::simple(Ident::new(
                    "Nat",
                    Span::new(start, self.prev_span().end),
                ))))
            }
            TokenKind::Unit => {
                self.advance();
                Some(TypeExpr::Unit(Span::new(start, self.prev_span().end)))
            }
            TokenKind::Void => {
                self.advance();
                Some(TypeExpr::Void(Span::new(start, self.prev_span().end)))
            }
            TokenKind::Prop => {
                self.advance();
                Some(TypeExpr::Prop(Span::new(start, self.prev_span().end)))
            }
            TokenKind::Forall => {
                self.advance();
                let name = self.parse_ident()?;
                self.expect(TokenKind::Dot)?;
                let body = self.parse_type()?;
                let end = body.span().end;
                Some(TypeExpr::Forall(
                    name,
                    Box::new(body),
                    Span::new(start, end),
                ))
            }
            TokenKind::Star => {
                // Pointer type: *T
                self.advance();
                let inner = self.parse_type_atom()?;
                let end = inner.span().end;
                Some(TypeExpr::Ptr(Box::new(inner), Span::new(start, end)))
            }
            TokenKind::Ref => {
                // Ref type: Ref<T> — first try as keyword, then parse generic args
                self.advance();
                if self.check(TokenKind::Lt) {
                    let args = self.parse_type_args()?;
                    if args.len() != 1 {
                        self.error(ParseErrorKind::Expected(
                            "exactly one type argument for Ref".to_string(),
                        ));
                        return Some(TypeExpr::Error(Span::new(start, self.prev_span().end)));
                    }
                    let end = self.prev_span().end;
                    Some(TypeExpr::Ref(
                        Box::new(args.into_iter().next().unwrap()),
                        Span::new(start, end),
                    ))
                } else {
                    // Just `Ref` without type args - treat as name
                    Some(TypeExpr::Path(Path::simple(Ident::new(
                        "Ref",
                        Span::new(start, self.prev_span().end),
                    ))))
                }
            }
            TokenKind::LParen => {
                self.advance();
                if self.check(TokenKind::RParen) {
                    self.advance();
                    Some(TypeExpr::Unit(Span::new(start, self.prev_span().end)))
                } else {
                    let first = self.parse_type()?;
                    if self.check(TokenKind::Comma) {
                        // Tuple type: (A, B, C) -> A * B * C
                        let mut types = vec![first];
                        while self.eat(TokenKind::Comma) {
                            if self.check(TokenKind::RParen) {
                                break;
                            }
                            types.push(self.parse_type()?);
                        }
                        self.expect(TokenKind::RParen)?;
                        // Fold into right-associative nested Product: A * (B * C)
                        let result = types
                            .into_iter()
                            .rev()
                            .reduce(|acc, ty| {
                                let span = Span::new(ty.span().start, acc.span().end);
                                TypeExpr::Product(Box::new(ty), Box::new(acc), span)
                            })
                            .unwrap();
                        Some(result)
                    } else {
                        // Parenthesized: (T)
                        let end = self.expect(TokenKind::RParen)?.end;
                        Some(TypeExpr::Paren(Box::new(first), Span::new(start, end)))
                    }
                }
            }
            TokenKind::Ident => {
                let path = self.parse_path()?;
                // Check for Ref<T> as identifier (backwards compat)
                if path.is_simple() && path.item_name().name == "Ref" && self.check(TokenKind::Lt) {
                    let args = self.parse_type_args()?;
                    if args.len() != 1 {
                        self.error(ParseErrorKind::Expected(
                            "exactly one type argument for Ref".to_string(),
                        ));
                        return Some(TypeExpr::Error(Span::new(start, self.prev_span().end)));
                    }
                    let end = self.prev_span().end;
                    Some(TypeExpr::Ref(
                        Box::new(args.into_iter().next().unwrap()),
                        Span::new(start, end),
                    ))
                } else if self.check(TokenKind::Lt) {
                    // Check for generic arguments
                    let args = self.parse_type_args()?;
                    let end = self.prev_span().end;
                    Some(TypeExpr::App(
                        Box::new(TypeExpr::Path(path)),
                        args,
                        Span::new(start, end),
                    ))
                } else {
                    Some(TypeExpr::Path(path))
                }
            }
            TokenKind::Bang => {
                self.advance();
                Some(TypeExpr::Void(Span::new(start, self.prev_span().end)))
            }
            _ => {
                self.error(ParseErrorKind::InvalidType);
                Some(TypeExpr::Error(self.current_span()))
            }
        }
    }

    pub(super) fn parse_type_args(&mut self) -> Option<Vec<TypeExpr>> {
        self.expect(TokenKind::Lt)?;

        let mut args = Vec::new();
        loop {
            if self.check(TokenKind::Gt) || self.at_eof() {
                break;
            }

            args.push(self.parse_type()?);

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::Gt)?;
        Some(args)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Type Definitions (Variants, Structs, Enums)
    // ─────────────────────────────────────────────────────────────────────────

    pub(super) fn parse_variants(&mut self) -> Option<Vec<Variant>> {
        let mut variants = Vec::new();

        // Skip leading pipe if present
        self.eat(TokenKind::Pipe);

        loop {
            if !self.check_ident() {
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

    pub(super) fn parse_struct_fields(&mut self) -> Option<Vec<Field>> {
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

    pub(super) fn parse_enum_variants(&mut self) -> Option<Vec<Variant>> {
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
