//! Operator and punctuation lexing.
//!
//! Handles multi-character operators (::, .., ==, =>, ->, etc.),
//! comments (line, doc, block), and single-character fallback.

use crate::error::{LexError, LexErrorKind};
use crate::span::Span;
use crate::token::TokenKind;

use super::Lexer;

impl<'a> Lexer<'a> {
    pub(super) fn colon(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some(':') {
            self.advance();
            TokenKind::ColonColon
        } else {
            TokenKind::Colon
        }
    }

    pub(super) fn dot(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('.') {
            self.advance();
            if self.peek() == Some('.') {
                self.advance();
                TokenKind::DotDotDot
            } else {
                TokenKind::DotDot
            }
        } else {
            TokenKind::Dot
        }
    }

    pub(super) fn eq(&mut self) -> TokenKind {
        self.advance();
        match self.peek() {
            Some('=') => {
                self.advance();
                TokenKind::EqEq
            }
            Some('>') => {
                self.advance();
                TokenKind::FatArrow
            }
            _ => TokenKind::Eq,
        }
    }

    pub(super) fn bang(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('=') {
            self.advance();
            TokenKind::Ne
        } else {
            TokenKind::Bang
        }
    }

    pub(super) fn lt(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('=') {
            self.advance();
            TokenKind::Le
        } else {
            TokenKind::Lt
        }
    }

    pub(super) fn gt(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('=') {
            self.advance();
            TokenKind::Ge
        } else {
            TokenKind::Gt
        }
    }

    pub(super) fn plus(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('+') {
            self.advance();
            TokenKind::PlusPlus
        } else {
            TokenKind::Plus
        }
    }

    pub(super) fn minus_or_arrow(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance();
        match self.peek() {
            Some('>') => {
                self.advance();
                TokenKind::Arrow
            }
            Some('-') => {
                // User likely meant a comment, suggest // instead
                self.advance();
                // Consume the rest of the line as if it were a comment
                self.advance_while(|c| c != '\n');
                self.errors.push(LexError::new(
                    Span::new(start, start + 2),
                    LexErrorKind::WrongCommentSyntax,
                ));
                // Return as a comment so parsing can continue
                TokenKind::LineComment
            }
            _ => TokenKind::Minus,
        }
    }

    pub(super) fn slash(&mut self) -> TokenKind {
        self.advance();
        match self.peek() {
            Some('/') => {
                self.advance();
                // Check for doc comment
                let is_doc = matches!(self.peek(), Some('/' | '!'));
                self.advance_while(|c| c != '\n');
                if is_doc {
                    TokenKind::DocComment
                } else {
                    TokenKind::LineComment
                }
            }
            Some('*') => self.block_comment(),
            _ => TokenKind::Slash,
        }
    }

    /// Lex a block comment (/* ... */), with nesting support.
    fn block_comment(&mut self) -> TokenKind {
        let start = self.pos - 1;
        self.advance();
        let mut depth = 1;
        while depth > 0 {
            match (self.peek(), self.peek_next()) {
                (None, _) => {
                    self.errors.push(LexError::new(
                        Span::new(start, self.pos),
                        LexErrorKind::UnterminatedBlockComment,
                    ));
                    return TokenKind::Error;
                }
                (Some('*'), Some('/')) => {
                    self.advance();
                    self.advance();
                    depth -= 1;
                }
                (Some('/'), Some('*')) => {
                    self.advance();
                    self.advance();
                    depth += 1;
                }
                _ => {
                    self.advance();
                }
            }
        }
        TokenKind::BlockComment
    }

    pub(super) fn amp(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('&') {
            self.advance();
            TokenKind::AmpAmp
        } else {
            TokenKind::Amp
        }
    }

    pub(super) fn pipe(&mut self) -> TokenKind {
        self.advance();
        match self.peek() {
            Some('|') => {
                self.advance();
                TokenKind::PipePipe
            }
            Some('>') => {
                self.advance();
                TokenKind::PipeRight
            }
            _ => TokenKind::Pipe,
        }
    }
}
