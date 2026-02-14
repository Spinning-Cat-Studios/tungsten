//! Lexer for the Tungsten language.
//!
//! Hand-written lexer that tokenizes source text into a stream of tokens,
//! with full support for trivia (whitespace, comments) preservation.

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

            // Single-character tokens or operators
            '(' => self.single(TokenKind::LParen),
            ')' => self.single(TokenKind::RParen),
            '{' => self.single(TokenKind::LBrace),
            '}' => self.single(TokenKind::RBrace),
            '[' => self.single(TokenKind::LBracket),
            ']' => self.single(TokenKind::RBracket),
            ',' => self.single(TokenKind::Comma),
            ';' => self.single(TokenKind::Semi),
            '@' => self.single(TokenKind::At),
            '#' => self.single(TokenKind::Hash),
            '?' => self.single(TokenKind::Question),
            '~' => self.single(TokenKind::Tilde),
            '^' => self.single(TokenKind::Caret),
            '%' => self.single(TokenKind::Percent),

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

    fn number(&mut self) -> TokenKind {
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

    fn string(&mut self) -> TokenKind {
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
                    if let Some(c) = self.peek() {
                        match c {
                            'n' | 'r' | 't' | '\\' | '\'' | '"' | '0' => {
                                self.advance();
                            }
                            'x' => {
                                // Hex escape: \xNN
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
                            'u' => {
                                // Unicode escape: \u{NNNNNN}
                                self.advance(); // consume 'u'
                                if self.peek() == Some('{') {
                                    self.advance(); // consume '{'
                                    let mut digit_count = 0;
                                    while let Some(c) = self.peek() {
                                        if c == '}' {
                                            self.advance();
                                            if digit_count == 0 {
                                                self.errors.push(LexError::new(
                                                    Span::new(self.pos - 3, self.pos),
                                                    LexErrorKind::InvalidUnicodeEscape,
                                                ));
                                            }
                                            break;
                                        } else if c.is_ascii_hexdigit() && digit_count < 6 {
                                            self.advance();
                                            digit_count += 1;
                                        } else {
                                            self.errors.push(LexError::new(
                                                Span::new(self.pos - 2 - digit_count, self.pos + 1),
                                                LexErrorKind::InvalidUnicodeEscape,
                                            ));
                                            // Skip to closing brace or end of string
                                            while let Some(c) = self.peek() {
                                                if c == '}' || c == '"' || c == '\n' {
                                                    break;
                                                }
                                                self.advance();
                                            }
                                            if self.peek() == Some('}') {
                                                self.advance();
                                            }
                                            break;
                                        }
                                    }
                                } else {
                                    self.errors.push(LexError::new(
                                        Span::new(self.pos - 2, self.pos),
                                        LexErrorKind::InvalidUnicodeEscape,
                                    ));
                                }
                            }
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
                Some(_) => {
                    self.advance();
                }
            }
        }
    }

    fn char_literal(&mut self) -> TokenKind {
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
                if let Some(c) = self.peek() {
                    match c {
                        'n' | 'r' | 't' | '\\' | '\'' | '"' | '0' => {
                            self.advance();
                        }
                        'x' => {
                            // Hex escape: \xNN
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
                        'u' => {
                            // Unicode escape: \u{NNNNNN}
                            self.advance(); // consume 'u'
                            if self.peek() == Some('{') {
                                self.advance(); // consume '{'
                                let mut digit_count = 0;
                                while let Some(c) = self.peek() {
                                    if c == '}' {
                                        self.advance();
                                        if digit_count == 0 {
                                            self.errors.push(LexError::new(
                                                Span::new(self.pos - 3, self.pos),
                                                LexErrorKind::InvalidUnicodeEscape,
                                            ));
                                        }
                                        break;
                                    } else if c.is_ascii_hexdigit() && digit_count < 6 {
                                        self.advance();
                                        digit_count += 1;
                                    } else {
                                        self.errors.push(LexError::new(
                                            Span::new(self.pos - 2 - digit_count, self.pos + 1),
                                            LexErrorKind::InvalidUnicodeEscape,
                                        ));
                                        // Skip to closing brace or end
                                        while let Some(c) = self.peek() {
                                            if c == '}' || c == '\'' || c == '\n' {
                                                break;
                                            }
                                            self.advance();
                                        }
                                        if self.peek() == Some('}') {
                                            self.advance();
                                        }
                                        break;
                                    }
                                }
                            } else {
                                self.errors.push(LexError::new(
                                    Span::new(self.pos - 2, self.pos),
                                    LexErrorKind::InvalidUnicodeEscape,
                                ));
                            }
                        }
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

    fn colon(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some(':') {
            self.advance();
            TokenKind::ColonColon
        } else {
            TokenKind::Colon
        }
    }

    fn dot(&mut self) -> TokenKind {
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

    fn eq(&mut self) -> TokenKind {
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

    fn bang(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('=') {
            self.advance();
            TokenKind::Ne
        } else {
            TokenKind::Bang
        }
    }

    fn lt(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('=') {
            self.advance();
            TokenKind::Le
        } else {
            TokenKind::Lt
        }
    }

    fn gt(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('=') {
            self.advance();
            TokenKind::Ge
        } else {
            TokenKind::Gt
        }
    }

    fn plus(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('+') {
            self.advance();
            TokenKind::PlusPlus
        } else {
            TokenKind::Plus
        }
    }

    fn minus_or_arrow(&mut self) -> TokenKind {
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

    fn slash(&mut self) -> TokenKind {
        self.advance();
        match self.peek() {
            Some('/') => {
                self.advance();
                // Check for doc comment
                let is_doc = matches!(self.peek(), Some('/') | Some('!'));
                self.advance_while(|c| c != '\n');
                if is_doc {
                    TokenKind::DocComment
                } else {
                    TokenKind::LineComment
                }
            }
            Some('*') => {
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
            _ => TokenKind::Slash,
        }
    }

    fn amp(&mut self) -> TokenKind {
        self.advance();
        if self.peek() == Some('&') {
            self.advance();
            TokenKind::AmpAmp
        } else {
            TokenKind::Amp
        }
    }

    fn pipe(&mut self) -> TokenKind {
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

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.advance();
        kind
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn peek(&self) -> Option<char> {
        self.source[self.pos as usize..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.pos as usize..].chars();
        chars.next();
        chars.next()
    }

    fn advance(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8() as u32;
        }
    }

    fn advance_while<F: Fn(char) -> bool>(&mut self, pred: F) {
        while let Some(c) = self.peek() {
            if pred(c) {
                self.advance();
            } else {
                break;
            }
        }
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
