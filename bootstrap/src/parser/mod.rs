//! Parser for the Tungsten language.
//!
//! Hand-written recursive descent parser with Pratt parsing for expressions.
//! Produces surface AST that can be elaborated into core terms.
//!
//! # Module Structure
//! - `items.rs` - Top-level items (functions, types, theorems, axioms)
//! - `types.rs` - Type expressions and type definitions
//! - `exprs.rs` - Expressions with Pratt parsing for operators
//! - `patterns.rs` - Pattern matching

use crate::ast::*;
use crate::error::{ParseError, ParseErrorKind, Suggestion};
use crate::lexer::Lexer;
use crate::span::{Span, Spanned};
use crate::token::{Token, TokenKind};
use crate::utils::find_best_suggestion;

mod exprs;
mod items;
mod patterns;
mod types;

/// Keywords for suggestion purposes.
const KEYWORDS: &[&str] = &[
    "fn", "let", "if", "else", "match", "for", "while", "return", "type", "struct", "enum",
    "theorem", "lemma", "axiom", "pub", "mod", "use", "extern", "true", "false", "sorry", "inl",
    "inr", "fst", "snd", "refl", "absurd", "as", "in", "Nat", "Bool", "Unit", "String", "Type",
    "Void", "Eq",
];

/// Parser for Tungsten source code.
pub struct Parser<'a> {
    /// Source text
    source: &'a str,
    /// Tokens (excluding trivia)
    tokens: Vec<Token>,
    /// Current position in token stream
    pos: usize,
    /// Collected errors
    errors: Vec<ParseError>,
    /// EOF token for when we're past the end
    eof_token: Token,
}

impl<'a> Parser<'a> {
    /// Create a new parser from source text.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        let lexer = Lexer::new(source);
        let (tokens, lex_errors) = lexer.tokenize();

        // Convert lex errors to parse errors, preserving specific error types
        let errors: Vec<_> = lex_errors
            .into_iter()
            .map(|e| {
                let kind = match e.kind {
                    crate::error::LexErrorKind::ReservedKeyword(kw) => {
                        ParseErrorKind::ReservedKeyword(kw)
                    }
                    _ => ParseErrorKind::UnexpectedToken(format!("{}", e)),
                };
                ParseError::new(e.span, kind)
            })
            .collect();

        let eof_token = Token::new(TokenKind::Eof, Span::empty(source.len() as u32));

        Self {
            source,
            tokens,
            pos: 0,
            errors,
            eof_token,
        }
    }

    /// Parse a complete source file.
    #[must_use]
    pub fn parse(mut self) -> (SourceFile, Vec<ParseError>) {
        let start = self.current_span().start;
        let mut items = Vec::new();

        while !self.at_eof() {
            if let Some(item) = self.parse_item() {
                items.push(item);
            } else {
                // Error recovery: skip to next item
                self.synchronize_to_item();
            }
        }

        let end = self.current_span().end;
        let file = SourceFile {
            items,
            span: Span::new(start, end),
        };
        (file, self.errors)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Parameters and Type Parameters
    // ─────────────────────────────────────────────────────────────────────────

    fn parse_optional_type_params(&mut self) -> Vec<TypeParam> {
        if !self.eat(TokenKind::Lt) {
            return Vec::new();
        }

        let mut params = Vec::new();
        loop {
            if self.check(TokenKind::Gt) || self.at_eof() {
                break;
            }

            if let Some(param) = self.parse_type_param() {
                params.push(param);
            }

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::Gt);
        params
    }

    fn parse_type_param(&mut self) -> Option<TypeParam> {
        let name = self.parse_ident()?;
        let span = name.span;
        Some(TypeParam {
            name,
            bounds: Vec::new(), // Future: parse trait bounds
            span,
        })
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        while !self.check(TokenKind::RParen) && !self.at_eof() {
            if let Some(param) = self.parse_param() {
                params.push(param);
            }

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RParen)?;
        Some(params)
    }

    fn parse_param(&mut self) -> Option<Param> {
        let start = self.current_span().start;
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let end = ty.span().end;

        Some(Param {
            pattern,
            ty,
            span: Span::new(start, end),
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn parse_ident(&mut self) -> Option<Ident> {
        if self.current().kind != TokenKind::Ident {
            // Don't emit "expected identifier" if the token is a reserved keyword,
            // since the lexer already reported that error.
            if !self.current().kind.is_reserved() {
                self.error(ParseErrorKind::Expected("identifier".to_string()));
            }
            return None;
        }

        let text = self.current_text().to_string();
        let span = self.current_span();
        self.advance();
        Some(Ident::new(text, span))
    }

    /// Parse a path: `ident` or `ident::ident::...`
    ///
    /// Used for qualified names in expressions, types, and patterns.
    fn parse_path(&mut self) -> Option<Path> {
        let start = self.current_span().start;

        let first = self.parse_ident()?;
        let mut segments = vec![first];

        // Parse additional `::ident` segments
        while self.eat(TokenKind::ColonColon) {
            let segment = self.parse_ident()?;
            segments.push(segment);
        }

        let end = segments.last().unwrap().span.end;
        Some(Path {
            segments,
            span: Span::new(start, end),
        })
    }

    fn parse_int_literal(&self, text: &str) -> u64 {
        let text = text.replace('_', "");
        if text.starts_with("0x") || text.starts_with("0X") {
            u64::from_str_radix(&text[2..], 16).unwrap_or(0)
        } else if text.starts_with("0o") || text.starts_with("0O") {
            u64::from_str_radix(&text[2..], 8).unwrap_or(0)
        } else if text.starts_with("0b") || text.starts_with("0B") {
            u64::from_str_radix(&text[2..], 2).unwrap_or(0)
        } else {
            text.parse().unwrap_or(0)
        }
    }

    fn unescape_string(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('\'') => result.push('\''),
                    Some('"') => result.push('"'),
                    Some('0') => result.push('\0'),
                    Some('x') => {
                        // Hex escape: \xNN
                        let hex: String = chars.by_ref().take(2).collect();
                        if hex.len() == 2 {
                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                result.push(byte as char);
                            } else {
                                // Invalid hex, just include literally
                                result.push_str("\\x");
                                result.push_str(&hex);
                            }
                        } else {
                            result.push_str("\\x");
                            result.push_str(&hex);
                        }
                    }
                    Some('u') => {
                        // Unicode escape: \u{NNNNNN}
                        if chars.peek() == Some(&'{') {
                            chars.next(); // consume '{'
                            let mut hex = String::new();
                            while let Some(&c) = chars.peek() {
                                if c == '}' {
                                    chars.next(); // consume '}'
                                    break;
                                } else if c.is_ascii_hexdigit() && hex.len() < 6 {
                                    hex.push(c);
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            if !hex.is_empty() {
                                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                    if let Some(ch) = char::from_u32(code) {
                                        result.push(ch);
                                    } else {
                                        // Invalid unicode code point
                                        result.push_str("\\u{");
                                        result.push_str(&hex);
                                        result.push('}');
                                    }
                                } else {
                                    result.push_str("\\u{");
                                    result.push_str(&hex);
                                    result.push('}');
                                }
                            } else {
                                result.push_str("\\u{}");
                            }
                        } else {
                            result.push_str("\\u");
                        }
                    }
                    Some(c) => {
                        result.push('\\');
                        result.push(c);
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof_token)
    }

    fn current_span(&self) -> Span {
        self.current().span
    }

    fn current_text(&self) -> &str {
        self.current().text(self.source)
    }

    /// Get the content of a string literal (without quotes).
    fn get_string_literal(&self) -> Option<String> {
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

    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            Span::empty(0)
        }
    }

    fn at_eof(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    fn check_ident(&self) -> bool {
        self.current().kind == TokenKind::Ident
    }

    fn check_ahead(&self, kind: TokenKind) -> bool {
        self.tokens
            .get(self.pos + 1)
            .map_or(false, |t| t.kind == kind)
    }

    /// Check n tokens ahead using a predicate.
    fn check_ahead_n(&self, n: usize, pred: impl FnOnce(TokenKind) -> bool) -> bool {
        self.tokens
            .get(self.pos + n)
            .map_or(false, |t| pred(t.kind))
    }

    /// Get the token n positions ahead.
    fn peek_n(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .unwrap_or(&self.tokens[self.tokens.len() - 1])
    }

    fn can_start_expr(&self) -> bool {
        self.current().kind.can_start_expr()
    }

    fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Span> {
        if self.check(kind) {
            let span = self.current_span();
            self.advance();
            Some(span)
        } else {
            self.error_expected(kind);
            None
        }
    }

    fn error(&mut self, kind: ParseErrorKind) {
        let span = self.current_span();
        let mut error = ParseError::new(span, kind);

        // Add keyword suggestion if the current token is an identifier that looks like a keyword
        if let TokenKind::Ident = self.current().kind {
            let text = self.current_text();
            if let Some(suggestion) = find_best_suggestion(text, KEYWORDS.iter().copied()) {
                error = error.with_suggestion(Suggestion::new(
                    span,
                    suggestion,
                    &format!("did you mean `{}`?", suggestion),
                ));
            }
        }

        self.errors.push(error);
    }

    fn error_expected(&mut self, kind: TokenKind) {
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
                    &format!("did you mean `{}`?", suggestion),
                ));
            }
        }

        self.errors.push(error);
    }

    fn synchronize_to_item(&mut self) {
        while !self.at_eof() {
            if self.current().kind.can_start_item() {
                return;
            }
            self.advance();
        }
    }
}

/// Parse a source string into an AST.
#[must_use]
pub fn parse(source: &str) -> (SourceFile, Vec<ParseError>) {
    Parser::new(source).parse()
}

#[cfg(test)]
mod tests;
