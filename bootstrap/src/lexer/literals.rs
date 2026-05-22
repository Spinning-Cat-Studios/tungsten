//! Literal token lexing (numbers, strings, character literals).
//!
//! Handles the full grammar for numeric literals (decimal, hex, octal, binary),
//! string literals with escape sequences, and character literals.

use crate::error::{LexError, LexErrorKind};
use crate::span::Span;
use crate::token::TokenKind;

use super::Lexer;

impl<'a> Lexer<'a> {
    pub(super) fn number(&mut self) -> TokenKind {
        let start = self.pos as usize;

        // Check for hex, octal, binary
        if self.peek() == Some('0') {
            self.advance();
            match self.peek() {
                Some('x' | 'X') => {
                    self.advance();
                    if !matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                        let text = &self.source[start..self.pos as usize];
                        self.errors.push(LexError::new(
                            Span::new(start as u32, self.pos),
                            LexErrorKind::InvalidNumber(text.to_string()),
                        ));
                        return TokenKind::Error;
                    }
                    self.advance_while(|c| c.is_ascii_hexdigit() || c == '_');
                    return TokenKind::IntLiteral;
                }
                Some('o' | 'O') => {
                    self.advance();
                    if !matches!(self.peek(), Some('0'..='7')) {
                        let text = &self.source[start..self.pos as usize];
                        self.errors.push(LexError::new(
                            Span::new(start as u32, self.pos),
                            LexErrorKind::InvalidNumber(text.to_string()),
                        ));
                        return TokenKind::Error;
                    }
                    self.advance_while(|c| matches!(c, '0'..='7' | '_'));
                    return TokenKind::IntLiteral;
                }
                Some('b' | 'B') => {
                    self.advance();
                    if !matches!(self.peek(), Some('0' | '1')) {
                        let text = &self.source[start..self.pos as usize];
                        self.errors.push(LexError::new(
                            Span::new(start as u32, self.pos),
                            LexErrorKind::InvalidNumber(text.to_string()),
                        ));
                        return TokenKind::Error;
                    }
                    self.advance_while(|c| matches!(c, '0' | '1' | '_'));
                    return TokenKind::IntLiteral;
                }
                _ => {}
            }
        }

        // Decimal number
        self.advance_while(|c| c.is_ascii_digit() || c == '_');
        TokenKind::IntLiteral
    }

    /// Lex an escape sequence after the `\` has already been consumed.
    ///
    /// `terminator` is the quote character (`'"'` for strings, `'\''` for char
    /// literals) used for error recovery in invalid unicode escapes.
    pub(super) fn lex_escape_sequence(&mut self, terminator: char) {
        if let Some(c) = self.peek() {
            match c {
                'n' | 'r' | 't' | '\\' | '\'' | '"' | '0' => {
                    self.advance();
                }
                'x' => self.lex_hex_escape(),
                'u' => self.lex_unicode_escape(terminator),
                _ => {
                    self.errors.push(LexError::new(
                        Span::new(self.pos - 1, self.pos + 1),
                        LexErrorKind::InvalidEscape(c),
                    ));
                    self.advance();
                }
            }
        }
    }

    /// Lex a hex escape `\xNN` after the `\` has been consumed.
    fn lex_hex_escape(&mut self) {
        self.advance(); // consume 'x'
        for i in 0..2 {
            if let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() {
                    self.advance();
                } else {
                    self.errors.push(LexError::new(
                        Span::new(self.pos - 2 - i, self.pos + 1),
                        LexErrorKind::InvalidHexEscape,
                    ));
                    break;
                }
            } else {
                self.errors.push(LexError::new(
                    Span::new(self.pos - 2 - i, self.pos),
                    LexErrorKind::InvalidHexEscape,
                ));
                break;
            }
        }
    }

    /// Lex a unicode escape `\u{NNNNNN}` after the `\` has been consumed.
    fn lex_unicode_escape(&mut self, terminator: char) {
        self.advance(); // consume 'u'
        if self.peek() != Some('{') {
            self.errors.push(LexError::new(
                Span::new(self.pos - 2, self.pos),
                LexErrorKind::InvalidUnicodeEscape,
            ));
            return;
        }
        self.advance(); // consume '{'
        let mut digit_count: u32 = 0;
        while let Some(c) = self.peek() {
            if c == '}' {
                self.advance();
                if digit_count == 0 {
                    self.errors.push(LexError::new(
                        Span::new(self.pos - 3, self.pos),
                        LexErrorKind::InvalidUnicodeEscape,
                    ));
                }
                return;
            } else if c.is_ascii_hexdigit() && digit_count < 6 {
                self.advance();
                digit_count += 1;
            } else {
                self.errors.push(LexError::new(
                    Span::new(self.pos - 2 - digit_count, self.pos + 1),
                    LexErrorKind::InvalidUnicodeEscape,
                ));
                // Skip to closing brace or terminator
                while let Some(c) = self.peek() {
                    if c == '}' || c == terminator || c == '\n' {
                        break;
                    }
                    self.advance();
                }
                if self.peek() == Some('}') {
                    self.advance();
                }
                return;
            }
        }
    }

    pub(super) fn string(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance(); // consume opening "

        loop {
            match self.peek() {
                None | Some('\n') => {
                    self.errors.push(LexError::new(
                        Span::new(start, self.pos),
                        LexErrorKind::UnterminatedString,
                    ));
                    return TokenKind::Error;
                }
                Some('"') => {
                    self.advance();
                    return TokenKind::StringLiteral;
                }
                Some('\\') => {
                    self.advance();
                    self.lex_escape_sequence('"');
                }
                Some(_) => {
                    self.advance();
                }
            }
        }
    }

    pub(super) fn char_literal(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance(); // consume opening '

        match self.peek() {
            None | Some('\n') => {
                self.errors.push(LexError::new(
                    Span::new(start, self.pos),
                    LexErrorKind::UnterminatedChar,
                ));
                return TokenKind::Error;
            }
            Some('\'') => {
                self.advance();
                self.errors.push(LexError::new(
                    Span::new(start, self.pos),
                    LexErrorKind::EmptyCharLiteral,
                ));
                return TokenKind::Error;
            }
            Some('\\') => {
                self.advance();
                self.lex_escape_sequence('\'');
            }
            Some(_) => {
                self.advance();
            }
        }

        // Expect closing '
        if self.peek() == Some('\'') {
            self.advance();
            TokenKind::CharLiteral
        } else {
            self.errors.push(LexError::new(
                Span::new(start, self.pos),
                LexErrorKind::UnterminatedChar,
            ));
            TokenKind::Error
        }
    }
}
