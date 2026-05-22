//! Parsing top-level items: functions, types, theorems, axioms.

use crate::ast::*;
use crate::error::ParseErrorKind;
use crate::span::{Span, Spanned};
use crate::token::TokenKind;

mod extern_fns;

use super::Parser;

impl Parser<'_> {
    /// Parse a visibility modifier: `pub`, `pub(crate)`, or nothing (private).
    ///
    /// This unconditionally consumes the `pub` token if present. Callers in
    /// non-item contexts (e.g., constructor/field parsing) should guard with
    /// `self.check(TokenKind::Pub)` before calling if they need to distinguish
    /// "no visibility" from "private".
    pub(super) fn parse_visibility(&mut self) -> Visibility {
        if self.eat(TokenKind::Pub) {
            if self.check(TokenKind::LParen) {
                self.advance(); // consume (
                if self.eat(TokenKind::Crate) {
                    if !self.eat(TokenKind::RParen) {
                        self.error(ParseErrorKind::Expected(
                            "`)` after `pub(crate`".to_string(),
                        ));
                    }
                    Visibility::Crate
                } else {
                    self.error(ParseErrorKind::Expected("`crate` after `pub(`".to_string()));
                    // Try to recover by consuming until )
                    while !self.check(TokenKind::RParen) && !self.at_eof() {
                        self.advance();
                    }
                    self.eat(TokenKind::RParen);
                    Visibility::Private // Error recovery: treat as private
                }
            } else {
                Visibility::Public
            }
        } else {
            Visibility::Private
        }
    }

    pub(super) fn parse_item(&mut self) -> Option<Item> {
        // Check for stray semicolons (common mistake from other languages)
        if self.check(TokenKind::Semi) {
            return self.handle_stray_semicolon();
        }

        let (item_kind, has_pub, has_pub_crate) = self.resolve_item_visibility();

        // Dispatch based on item kind (shared for both pub and non-pub paths)
        match item_kind {
            TokenKind::Mod => Some(Item::Mod(self.parse_mod_decl()?)),
            TokenKind::Use => Some(Item::Use(self.parse_use_decl()?)),
            TokenKind::Fn => Some(Item::Function(self.parse_function()?)),
            TokenKind::Type => self.parse_type_item(),
            TokenKind::Struct => Some(Item::TypeDef(self.parse_struct()?)),
            TokenKind::Enum => Some(Item::TypeDef(self.parse_enum()?)),
            TokenKind::Theorem => Some(Item::Theorem(self.parse_theorem()?)),
            TokenKind::Lemma => Some(Item::Lemma(self.parse_theorem()?)),
            TokenKind::Axiom => Some(Item::Axiom(self.parse_axiom()?)),
            TokenKind::Extern => Some(Item::ExternFn(self.parse_extern_fn()?)),
            _ if has_pub || has_pub_crate => self.report_invalid_pub_item(item_kind, has_pub_crate),
            _ => {
                self.error(ParseErrorKind::Expected("item".to_string()));
                None
            }
        }
    }

    /// Determine item visibility and peek at the item kind token.
    fn resolve_item_visibility(&self) -> (TokenKind, bool, bool) {
        let has_pub = self.check(TokenKind::Pub);
        let has_pub_crate = has_pub && self.check_ahead(TokenKind::LParen);

        let item_kind = if has_pub_crate {
            self.peek_past_pub_crate()
        } else if has_pub {
            self.peek_ahead().kind
        } else {
            self.current().kind
        };

        (item_kind, has_pub, has_pub_crate)
    }

    /// Handle a stray semicolon at the top level with a helpful suggestion.
    fn handle_stray_semicolon(&mut self) -> Option<Item> {
        let span = self.current_span();
        self.errors.push(crate::error::ParseError::new(
            span,
            crate::error::ParseErrorKind::UnexpectedToken(";".to_string()),
        ));
        self.errors.last_mut().map(|e| {
            e.suggestions.push(crate::error::Suggestion::new(
                span,
                "",
                "Tungsten declarations don't require trailing semicolons; remove the `;`",
            ));
        });
        self.advance();
        if !self.at_eof() {
            self.parse_item()
        } else {
            None
        }
    }

    /// Report an error for invalid use of visibility modifier.
    fn report_invalid_pub_item(
        &mut self,
        item_kind: TokenKind,
        has_pub_crate: bool,
    ) -> Option<Item> {
        let span = self.current_span();
        let message = if item_kind == TokenKind::Let {
            "item that can have visibility"
        } else {
            "item after visibility modifier"
        };
        let suggestion_span = if item_kind == TokenKind::Let && has_pub_crate {
            Span::new(self.peek_n(0).span.start, self.peek_n(3).span.end)
        } else {
            span
        };
        self.errors.push(crate::error::ParseError::new(
            span,
            crate::error::ParseErrorKind::Expected(message.to_string()),
        ));
        self.errors.last_mut().map(|e| {
            e.suggestions.push(crate::error::Suggestion::new(
                suggestion_span,
                "",
                "visibility modifiers can only be applied to: fn, type, struct, enum, mod, use, theorem, lemma, axiom, extern",
            ));
        });
        None
    }

    /// Peek past `pub(crate)` to see what token follows.
    fn peek_past_pub_crate(&self) -> TokenKind {
        // Current: pub, +1: (, +2: crate, +3: ), +4: item
        self.peek_n(4).kind
    }

    /// Peek one token ahead.
    fn peek_ahead(&self) -> &crate::token::Token {
        self.peek_n(1)
    }

    pub(super) fn parse_function(&mut self) -> Option<FunctionDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Fn)?;

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();
        let params = self.parse_params()?;

        let return_type = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = self.parse_block_expr()?;
        let end = body.span().end;

        Some(FunctionDef {
            visibility,
            name,
            type_params,
            params,
            return_type,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_theorem(&mut self) -> Option<TheoremDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        // Skip theorem or lemma keyword
        self.advance();

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();
        let params = if self.check(TokenKind::LParen) {
            self.parse_params()?
        } else {
            Vec::new()
        };

        // Accept both `:` and `->` for the proposition
        if !self.eat(TokenKind::Colon) && !self.eat(TokenKind::Arrow) {
            self.error(ParseErrorKind::Expected(
                "`:` or `->` before proposition".to_string(),
            ));
            return None;
        }
        let prop = self.parse_type()?;

        let body = if self.check(TokenKind::LBrace) {
            self.parse_block_expr()?
        } else if self.eat(TokenKind::Eq) {
            self.parse_expr()?
        } else {
            self.error(ParseErrorKind::Expected(
                "block or `=` for proof body".to_string(),
            ));
            return None;
        };

        let end = body.span().end;

        Some(TheoremDef {
            visibility,
            name,
            type_params,
            params,
            prop,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_axiom(&mut self) -> Option<AxiomDef> {
        let start = self.current_span().start;

        // Parse visibility
        let visibility = self.parse_visibility();

        self.expect(TokenKind::Axiom)?;

        let name = self.parse_ident()?;
        let type_params = self.parse_optional_type_params();
        let params = if self.check(TokenKind::LParen) {
            self.parse_params()?
        } else {
            Vec::new()
        };

        // Accept both `:` and `->` for the proposition
        if !self.eat(TokenKind::Colon) && !self.eat(TokenKind::Arrow) {
            self.error(ParseErrorKind::Expected(
                "`:` or `->` before proposition".to_string(),
            ));
            return None;
        }
        let prop = self.parse_type()?;
        let end = prop.span().end;

        Some(AxiomDef {
            visibility,
            name,
            type_params,
            params,
            prop,
            span: Span::new(start, end),
        })
    }
}
