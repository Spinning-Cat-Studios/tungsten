//! Parsing `#[partial]` and `#[decreasing(arg)]` (ADR 29.6.26e).
//!
//! Attributes are consumed before the visibility modifier, so the item
//! dispatcher's token peeking sees `fn`/`theorem` where it always did. An
//! unrecognised attribute is an **error**, not a skipped token: silently
//! ignoring `#[partal]` would leave a definition termination-checked while its
//! author believed it opted out, which is the one failure mode an escape hatch
//! must not have.

use crate::ast::TerminationAttrs;
use crate::error::ParseErrorKind;
use crate::token::TokenKind;

use crate::parser::Parser;

impl Parser<'_> {
    /// Consume every leading `#[…]` attribute, folding them into one record.
    pub(in crate::parser) fn parse_termination_attrs(&mut self) -> TerminationAttrs {
        let mut attrs = TerminationAttrs::default();
        while self.check(TokenKind::Hash) && self.check_ahead(TokenKind::LBracket) {
            self.advance(); // #
            self.advance(); // [
            self.parse_one_attribute(&mut attrs);
            if !self.eat(TokenKind::RBracket) {
                self.error(ParseErrorKind::Expected("`]` after attribute".to_string()));
                self.skip_to_attribute_end();
            }
        }
        attrs
    }

    /// Parse the body of a single `#[…]`, recording it in `attrs`.
    fn parse_one_attribute(&mut self, attrs: &mut TerminationAttrs) {
        let Some(name) = self.parse_ident() else {
            self.skip_to_attribute_end();
            return;
        };
        match name.name.as_str() {
            "partial" => attrs.partial = true,
            "decreasing" => self.parse_decreasing_argument(attrs),
            _ => {
                self.errors.push(crate::error::ParseError::new(
                    name.span,
                    ParseErrorKind::Expected("a known attribute".to_string()),
                ));
                if let Some(error) = self.errors.last_mut() {
                    error.suggestions.push(crate::error::Suggestion::new(
                        name.span,
                        "",
                        "the attributes Tungsten understands are `#[partial]` and \
                         `#[decreasing(arg)]`",
                    ));
                }
                self.skip_to_attribute_end();
            }
        }
    }

    /// Parse the `(arg)` of `#[decreasing(arg)]`.
    fn parse_decreasing_argument(&mut self, attrs: &mut TerminationAttrs) {
        if !self.eat(TokenKind::LParen) {
            self.error(ParseErrorKind::Expected(
                "`(` after `decreasing`".to_string(),
            ));
            self.skip_to_attribute_end();
            return;
        }
        attrs.decreasing = self.parse_ident();
        if !self.eat(TokenKind::RParen) {
            self.error(ParseErrorKind::Expected(
                "`)` after the decreasing parameter".to_string(),
            ));
            self.skip_to_attribute_end();
        }
    }

    /// Error recovery: run to the closing `]` so the item after it still parses.
    fn skip_to_attribute_end(&mut self) {
        while !self.check(TokenKind::RBracket) && !self.at_eof() {
            self.advance();
        }
    }
}
