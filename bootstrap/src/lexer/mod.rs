//! Lexer for the Tungsten language.
//!
//! Hand-written lexer that tokenizes source text into a stream of tokens,
//! with full support for trivia (whitespace, comments) preservation.

mod literals;
mod operators;

use crate::error::{LexError, LexErrorKind};
use crate::span::Span;
use crate::token::{keyword_from_str, Token, TokenKind};

/// Lexer for Tungsten source code.
pub struct Lexer<'a> {
    /// Source text being lexed
    source: &'a str,
    /// Current byte position
    pos: u32,
    /// Collected errors
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            errors: Vec::new(),
        }
    }

    /// Tokenize the entire source, returning all tokens including trivia.
    #[must_use]
    pub fn tokenize_all(mut self) -> (Vec<Token>, Vec<LexError>) {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        (tokens, self.errors)
    }

    /// Tokenize the source, filtering out trivia tokens.
    #[must_use]
    pub fn tokenize(self) -> (Vec<Token>, Vec<LexError>) {
        let (tokens, errors) = self.tokenize_all();
        let filtered: Vec<_> = tokens.into_iter().filter(|t| !t.is_trivia()).collect();
        (filtered, errors)
    }

    /// Get the next token.
    pub fn next_token(&mut self) -> Token {
        let start = self.pos;

        let Some(c) = self.peek() else {
            return Token::new(TokenKind::Eof, Span::empty(self.pos));
        };

        let kind = match c {
            // Whitespace
            c if c.is_ascii_whitespace() => self.whitespace(),

            // Identifiers and keywords
            c if is_ident_start(c) => self.ident_or_keyword(),

            // Numbers
            '0'..='9' => self.number(),

            // Strings
            '"' => self.string(),

            // Characters
            '\'' => self.char_literal(),

            // Single-character punctuation
            '(' | ')' | '{' | '}' | '[' | ']' | ',' | ';' | '@' | '#' | '?' | '~' | '^' | '%' => {
                self.single(single_char_token(c).unwrap())
            }

            // Two-character operators or single
            ':' => self.colon(),
            '.' => self.dot(),
            '=' => self.eq(),
            '!' => self.bang(),
            '<' => self.lt(),
            '>' => self.gt(),
            '+' => self.plus(),
            '-' => self.minus_or_arrow(),
            '*' => self.single(TokenKind::Star),
            '/' => self.slash(),
            '&' => self.amp(),
            '|' => self.pipe(),

            // Unknown character
            _ => {
                self.advance();
                self.errors.push(LexError::new(
                    Span::new(start, self.pos),
                    LexErrorKind::UnexpectedChar(c),
                ));
                TokenKind::Error
            }
        };

        Token::new(kind, Span::new(start, self.pos))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Token handlers
    // ─────────────────────────────────────────────────────────────────────────

    fn whitespace(&mut self) -> TokenKind {
        self.advance_while(|c| c.is_ascii_whitespace());
        TokenKind::Whitespace
    }

    fn ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos as usize;
        self.advance_while(is_ident_continue);
        let text = &self.source[start..self.pos as usize];

        // Check for keywords
        if let Some(kw) = keyword_from_str(text) {
            // Check if it's a reserved keyword (error)
            if kw.is_reserved() {
                self.errors.push(LexError::new(
                    Span::new(start as u32, self.pos),
                    LexErrorKind::ReservedKeyword(text.to_string()),
                ));
            }
            kw
        } else if text == "_" {
            TokenKind::Underscore
        } else {
            TokenKind::Ident
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.advance();
        kind
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.source[self.pos as usize..].chars().next()
    }

    pub(crate) fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.pos as usize..].chars();
        chars.next();
        chars.next()
    }

    pub(crate) fn advance(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8() as u32;
        }
    }

    pub(crate) fn advance_while<F: Fn(char) -> bool>(&mut self, pred: F) {
        while let Some(c) = self.peek() {
            if pred(c) {
                self.advance();
            } else {
                break;
            }
        }
    }
}

/// Map a single character to its token kind, or None if it's not a simple punctuation token.
fn single_char_token(c: char) -> Option<TokenKind> {
    match c {
        '(' => Some(TokenKind::LParen),
        ')' => Some(TokenKind::RParen),
        '{' => Some(TokenKind::LBrace),
        '}' => Some(TokenKind::RBrace),
        '[' => Some(TokenKind::LBracket),
        ']' => Some(TokenKind::RBracket),
        ',' => Some(TokenKind::Comma),
        ';' => Some(TokenKind::Semi),
        '@' => Some(TokenKind::At),
        '#' => Some(TokenKind::Hash),
        '?' => Some(TokenKind::Question),
        '~' => Some(TokenKind::Tilde),
        '^' => Some(TokenKind::Caret),
        '%' => Some(TokenKind::Percent),
        _ => None,
    }
}

/// Check if a character can start an identifier.
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Check if a character can continue an identifier.
fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests;
