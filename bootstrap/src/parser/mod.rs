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
use crate::error::{ParseError, ParseErrorKind};
use crate::lexer::Lexer;
use crate::span::{Span, Spanned};
use crate::token::{Token, TokenKind};

mod exprs;
mod imports;
mod items;
mod literals;
mod patterns;
mod token_nav;
mod token_support;
mod type_defs;
mod types;

/// Keywords for suggestion purposes.
pub(crate) const KEYWORDS: &[&str] = &[
    "fn", "let", "if", "else", "match", "for", "while", "return", "type", "struct", "enum",
    "theorem", "lemma", "axiom", "pub", "mod", "use", "extern", "true", "false", "sorry", "inl",
    "inr", "fst", "snd", "refl", "absurd", "as", "in", "Nat", "Bool", "Unit", "String", "Type",
    "Void", "Eq",
];

/// Parser for Tungsten source code.
pub struct Parser<'a> {
    /// Source text
    pub(crate) source: &'a str,
    /// Tokens (excluding trivia)
    pub(crate) tokens: Vec<Token>,
    /// Current position in token stream
    pub(crate) pos: usize,
    /// Collected errors
    pub(crate) errors: Vec<ParseError>,
    /// EOF token for when we're past the end
    pub(crate) eof_token: Token,
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
}

/// Parse a source string into an AST.
#[must_use]
pub fn parse(source: &str) -> (SourceFile, Vec<ParseError>) {
    Parser::new(source).parse()
}

#[cfg(test)]
mod tests;
