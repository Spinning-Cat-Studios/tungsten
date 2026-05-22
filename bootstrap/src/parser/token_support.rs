//! Error reporting, recovery, and extended lookahead for the parser.
//!
//! Error methods: `error`, `error_expected`, `synchronize_to_item`.
//! Extended lookahead: `get_string_literal`, `check_ahead_n`, `peek_n`.

use crate::error::{ParseError, ParseErrorKind, Suggestion};
use crate::span::Span;
use crate::token::TokenKind;
use crate::utils::find_best_suggestion;

use super::{Parser, KEYWORDS};

impl<'a> Parser<'a> {
    /// Get the content of a string literal (without quotes).
    pub(crate) fn get_string_literal(&self) -> Option<String> {
        if self.current().kind != TokenKind::StringLiteral {
            return None;
        }
        let text = self.current_text();
        // Remove surrounding quotes
        if text.len() >= 2 {
            Some(self.unescape_string(&text[1..text.len() - 1]))
        } else {
            Some(String::new())
        }
    }

    /// Check n tokens ahead using a predicate.
    pub(crate) fn check_ahead_n(&self, n: usize, pred: impl FnOnce(TokenKind) -> bool) -> bool {
        self.tokens
            .get(self.pos + n)
            .map_or(false, |t| pred(t.kind))
    }

    /// Get the token n positions ahead.
    pub(crate) fn peek_n(&self, n: usize) -> &crate::token::Token {
        self.tokens
            .get(self.pos + n)
            .unwrap_or(&self.tokens[self.tokens.len() - 1])
    }

    pub(crate) fn error(&mut self, kind: ParseErrorKind) {
        let span = self.current_span();
        let mut error = ParseError::new(span, kind);

        // Add keyword suggestion if the current token is an identifier that looks like a keyword
        if let TokenKind::Ident = self.current().kind {
            let text = self.current_text();
            if let Some(suggestion) = find_best_suggestion(text, KEYWORDS.iter().copied()) {
                error = error.with_suggestion(Suggestion::new(
                    span,
                    suggestion,
                    format!("did you mean `{}`?", suggestion),
                ));
            }
        }

        self.errors.push(error);
    }

    pub(crate) fn error_expected(&mut self, kind: TokenKind) {
        let span = self.current_span();
        let found = self.current_text().to_string();
        let mut error = ParseError::new(span, ParseErrorKind::UnexpectedToken(found.clone()))
            .with_expected(vec![format!("`{}`", kind)]);

        // Add keyword suggestion if the token looks like a misspelled keyword
        if let TokenKind::Ident = self.current().kind {
            if let Some(suggestion) = find_best_suggestion(&found, KEYWORDS.iter().copied()) {
                error = error.with_suggestion(Suggestion::new(
                    span,
                    suggestion,
                    format!("did you mean `{}`?", suggestion),
                ));
            }
        }

        self.errors.push(error);
    }

    pub(crate) fn synchronize_to_item(&mut self) {
        while !self.at_eof() {
            if self.current().kind.can_start_item() {
                return;
            }
            self.advance();
        }
    }
}
