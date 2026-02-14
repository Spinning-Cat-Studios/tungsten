//! Token types for the Tungsten lexer.
//!
//! Defines the ~50 token kinds used by the lexer and parser.

use crate::span::Span;
use serde::{Deserialize, Serialize};
use std::fmt;

/// All token kinds in Tungsten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenKind {
    // ─────────────────────────────────────────────────────────────────────────
    // Literals
    // ─────────────────────────────────────────────────────────────────────────
    /// Integer literal: `42`, `0x2A`, `0b101010`, `0o52`
    IntLiteral,
    /// String literal: `"hello"`
    StringLiteral,
    /// Character literal: `'a'`
    CharLiteral,

    // ─────────────────────────────────────────────────────────────────────────
    // Identifiers
    // ─────────────────────────────────────────────────────────────────────────
    /// Identifier: `foo`, `Bar`, `_baz`
    Ident,
    /// Underscore: `_` (used as wildcard pattern)
    Underscore,

    // ─────────────────────────────────────────────────────────────────────────
    // Keywords - Core
    // ─────────────────────────────────────────────────────────────────────────
    /// `fn`
    Fn,
    /// `let`
    Let,
    /// `if`
    If,
    /// `else`
    Else,
    /// `match`
    Match,
    /// `return`
    Return,
    /// `type`
    Type,
    /// `struct`
    Struct,
    /// `enum`
    Enum,
    /// `mod`
    Mod,
    /// `true`
    True,
    /// `false`
    False,

    // ─────────────────────────────────────────────────────────────────────────
    // Keywords - Proof
    // ─────────────────────────────────────────────────────────────────────────
    /// `theorem`
    Theorem,
    /// `lemma`
    Lemma,
    /// `axiom`
    Axiom,
    /// `by`
    By,
    /// `have`
    Have,
    /// `show`
    Show,
    /// `assume`
    Assume,
    /// `forall`
    Forall,
    /// `exists`
    Exists,
    /// `Prop`
    Prop,
    /// `sorry`
    Sorry,
    /// `refl` - reflexivity proof
    Refl,

    // ─────────────────────────────────────────────────────────────────────────
    // Keywords - Types
    // ─────────────────────────────────────────────────────────────────────────
    /// `Bool`
    Bool,
    /// `Nat`
    Nat,
    /// `Unit`
    Unit,
    /// `Void`
    Void,

    // ─────────────────────────────────────────────────────────────────────────
    // Reserved Keywords (for future use)
    // ─────────────────────────────────────────────────────────────────────────
    /// `async`
    Async,
    /// `await`
    Await,
    /// `pub`
    Pub,
    /// `use`
    Use,
    /// `impl`
    Impl,
    /// `trait`
    Trait,
    /// `where`
    Where,
    /// `self`
    SelfLower,
    /// `Self`
    SelfUpper,
    /// `mut`
    Mut,
    /// `ref`
    Ref,
    /// `move`
    Move,
    /// `loop`
    Loop,
    /// `while`
    While,
    /// `for`
    For,
    /// `in`
    In,
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `const`
    Const,
    /// `static`
    Static,
    /// `extern`
    Extern,
    /// `crate`
    Crate,
    /// `super`
    Super,
    /// `dyn`
    Dyn,
    /// `unsafe`
    Unsafe,
    /// `as`
    As,

    // ─────────────────────────────────────────────────────────────────────────
    // Delimiters
    // ─────────────────────────────────────────────────────────────────────────
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `<`
    Lt,
    /// `>`
    Gt,

    // ─────────────────────────────────────────────────────────────────────────
    // Punctuation
    // ─────────────────────────────────────────────────────────────────────────
    /// `,`
    Comma,
    /// `;`
    Semi,
    /// `:`
    Colon,
    /// `::`
    ColonColon,
    /// `.`
    Dot,
    /// `..`
    DotDot,
    /// `...`
    DotDotDot,
    /// `=>`
    FatArrow,
    /// `->`
    Arrow,
    /// `@`
    At,
    /// `#`
    Hash,
    /// `?`
    Question,

    // ─────────────────────────────────────────────────────────────────────────
    // Operators
    // ─────────────────────────────────────────────────────────────────────────
    /// `=`
    Eq,
    /// `==`
    EqEq,
    /// `!=`
    Ne,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `&`
    Amp,
    /// `&&`
    AmpAmp,
    /// `|`
    Pipe,
    /// `||`
    PipePipe,
    /// `|>`
    PipeRight,
    /// `!`
    Bang,
    /// `^`
    Caret,
    /// `~`
    Tilde,
    /// `++`
    PlusPlus,

    // ─────────────────────────────────────────────────────────────────────────
    // Trivia
    // ─────────────────────────────────────────────────────────────────────────
    /// Whitespace (spaces, tabs, newlines)
    Whitespace,
    /// Line comment: `// ...`
    LineComment,
    /// Block comment: `/* ... */`
    BlockComment,
    /// Doc comment: `/// ...` or `//! ...`
    DocComment,

    // ─────────────────────────────────────────────────────────────────────────
    // Special
    // ─────────────────────────────────────────────────────────────────────────
    /// End of file
    Eof,
    /// Lexer error token
    Error,
}

impl TokenKind {
    /// Check if this token is trivia (whitespace or comment).
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace
                | TokenKind::LineComment
                | TokenKind::BlockComment
                | TokenKind::DocComment
        )
    }

    /// Check if this token is a keyword.
    #[must_use]
    pub const fn is_keyword(self) -> bool {
        matches!(
            self,
            TokenKind::Fn
                | TokenKind::Let
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::Match
                | TokenKind::Return
                | TokenKind::Type
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Theorem
                | TokenKind::Lemma
                | TokenKind::Axiom
                | TokenKind::By
                | TokenKind::Have
                | TokenKind::Show
                | TokenKind::Assume
                | TokenKind::Forall
                | TokenKind::Exists
                | TokenKind::Prop
                | TokenKind::Sorry
                | TokenKind::Bool
                | TokenKind::Nat
                | TokenKind::Unit
                | TokenKind::Void
                | TokenKind::Extern
                | TokenKind::Ref
                | TokenKind::Pub
        )
    }

    /// Check if this token is a reserved keyword (not yet implemented).
    #[must_use]
    pub const fn is_reserved(self) -> bool {
        matches!(
            self,
            TokenKind::Async
                | TokenKind::Await
                | TokenKind::Impl
                | TokenKind::Trait
                | TokenKind::Where
                | TokenKind::SelfLower
                | TokenKind::SelfUpper
                | TokenKind::Mut
                | TokenKind::Move
                | TokenKind::Loop
                | TokenKind::While
                | TokenKind::For
                | TokenKind::In
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Const
                | TokenKind::Static
                | TokenKind::Super
                | TokenKind::Dyn
                | TokenKind::Unsafe
                | TokenKind::As
        )
    }

    /// Check if this is a literal token.
    #[must_use]
    pub const fn is_literal(self) -> bool {
        matches!(
            self,
            TokenKind::IntLiteral
                | TokenKind::StringLiteral
                | TokenKind::CharLiteral
                | TokenKind::True
                | TokenKind::False
        )
    }

    /// Check if this token can start an expression.
    #[must_use]
    pub const fn can_start_expr(self) -> bool {
        matches!(
            self,
            TokenKind::Ident
                | TokenKind::IntLiteral
                | TokenKind::StringLiteral
                | TokenKind::CharLiteral
                | TokenKind::True
                | TokenKind::False
                | TokenKind::LParen
                | TokenKind::LBrace
                | TokenKind::LBracket
                | TokenKind::If
                | TokenKind::Match
                | TokenKind::Fn
                | TokenKind::Forall
                | TokenKind::Exists
                | TokenKind::Bang
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Amp
                | TokenKind::Pipe
                | TokenKind::Sorry
                | TokenKind::Have
        )
    }

    /// Check if this token can start an item.
    #[must_use]
    pub const fn can_start_item(self) -> bool {
        matches!(
            self,
            TokenKind::Fn
                | TokenKind::Type
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Theorem
                | TokenKind::Lemma
                | TokenKind::Axiom
                | TokenKind::Extern
        )
    }

    /// Get the keyword for this token kind, if any.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TokenKind::IntLiteral => "<int>",
            TokenKind::StringLiteral => "<string>",
            TokenKind::CharLiteral => "<char>",
            TokenKind::Ident => "<identifier>",
            TokenKind::Underscore => "_",
            TokenKind::Fn => "fn",
            TokenKind::Let => "let",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::Match => "match",
            TokenKind::Return => "return",
            TokenKind::Type => "type",
            TokenKind::Struct => "struct",
            TokenKind::Enum => "enum",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::Theorem => "theorem",
            TokenKind::Lemma => "lemma",
            TokenKind::Axiom => "axiom",
            TokenKind::By => "by",
            TokenKind::Have => "have",
            TokenKind::Show => "show",
            TokenKind::Assume => "assume",
            TokenKind::Forall => "forall",
            TokenKind::Exists => "exists",
            TokenKind::Prop => "Prop",
            TokenKind::Sorry => "sorry",
            TokenKind::Refl => "refl",
            TokenKind::Bool => "Bool",
            TokenKind::Nat => "Nat",
            TokenKind::Unit => "Unit",
            TokenKind::Void => "Void",
            TokenKind::Async => "async",
            TokenKind::Await => "await",
            TokenKind::Pub => "pub",
            TokenKind::Use => "use",
            TokenKind::Mod => "mod",
            TokenKind::Impl => "impl",
            TokenKind::Trait => "trait",
            TokenKind::Where => "where",
            TokenKind::SelfLower => "self",
            TokenKind::SelfUpper => "Self",
            TokenKind::Mut => "mut",
            TokenKind::Ref => "ref",
            TokenKind::Move => "move",
            TokenKind::Loop => "loop",
            TokenKind::While => "while",
            TokenKind::For => "for",
            TokenKind::In => "in",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::Const => "const",
            TokenKind::Static => "static",
            TokenKind::Extern => "extern",
            TokenKind::Crate => "crate",
            TokenKind::Super => "super",
            TokenKind::Dyn => "dyn",
            TokenKind::Unsafe => "unsafe",
            TokenKind::As => "as",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Lt => "<",
            TokenKind::Gt => ">",
            TokenKind::Comma => ",",
            TokenKind::Semi => ";",
            TokenKind::Colon => ":",
            TokenKind::ColonColon => "::",
            TokenKind::Dot => ".",
            TokenKind::DotDot => "..",
            TokenKind::DotDotDot => "...",
            TokenKind::FatArrow => "=>",
            TokenKind::Arrow => "->",
            TokenKind::At => "@",
            TokenKind::Hash => "#",
            TokenKind::Question => "?",
            TokenKind::Eq => "=",
            TokenKind::EqEq => "==",
            TokenKind::Ne => "!=",
            TokenKind::Le => "<=",
            TokenKind::Ge => ">=",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::Amp => "&",
            TokenKind::AmpAmp => "&&",
            TokenKind::Pipe => "|",
            TokenKind::PipePipe => "||",
            TokenKind::PipeRight => "|>",
            TokenKind::Bang => "!",
            TokenKind::Caret => "^",
            TokenKind::Tilde => "~",
            TokenKind::PlusPlus => "++",
            TokenKind::Whitespace => "<whitespace>",
            TokenKind::LineComment => "<line comment>",
            TokenKind::BlockComment => "<block comment>",
            TokenKind::DocComment => "<doc comment>",
            TokenKind::Eof => "<eof>",
            TokenKind::Error => "<error>",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A token with its kind and source span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// The kind of token
    pub kind: TokenKind,
    /// Span in source text
    pub span: Span,
}

impl Token {
    /// Create a new token.
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Get the text of this token from the source.
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        self.span.text(source)
    }

    /// Check if this is an EOF token.
    #[must_use]
    pub const fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
    }

    /// Check if this is a trivia token.
    #[must_use]
    pub const fn is_trivia(&self) -> bool {
        self.kind.is_trivia()
    }
}

impl crate::span::Spanned for Token {
    fn span(&self) -> Span {
        self.span
    }
}

/// Look up a keyword from its string representation.
#[must_use]
pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
    Some(match s {
        // Core keywords
        "fn" => TokenKind::Fn,
        "let" => TokenKind::Let,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "match" => TokenKind::Match,
        "return" => TokenKind::Return,
        "type" => TokenKind::Type,
        "struct" => TokenKind::Struct,
        "enum" => TokenKind::Enum,
        "true" => TokenKind::True,
        "false" => TokenKind::False,

        // Proof keywords
        "theorem" => TokenKind::Theorem,
        "lemma" => TokenKind::Lemma,
        "axiom" => TokenKind::Axiom,
        "by" => TokenKind::By,
        "have" => TokenKind::Have,
        "show" => TokenKind::Show,
        "assume" => TokenKind::Assume,
        "forall" => TokenKind::Forall,
        "exists" => TokenKind::Exists,
        "Prop" => TokenKind::Prop,
        "sorry" => TokenKind::Sorry,
        "refl" => TokenKind::Refl,

        // Type keywords
        "Bool" => TokenKind::Bool,
        "Nat" => TokenKind::Nat,
        "Unit" => TokenKind::Unit,
        "Void" => TokenKind::Void,

        // Module keywords
        "mod" => TokenKind::Mod,
        "pub" => TokenKind::Pub,

        // Reserved keywords
        "async" => TokenKind::Async,
        "await" => TokenKind::Await,
        "use" => TokenKind::Use,
        "impl" => TokenKind::Impl,
        "trait" => TokenKind::Trait,
        "where" => TokenKind::Where,
        "self" => TokenKind::SelfLower,
        "Self" => TokenKind::SelfUpper,
        "mut" => TokenKind::Mut,
        "ref" => TokenKind::Ref,
        "move" => TokenKind::Move,
        "loop" => TokenKind::Loop,
        "while" => TokenKind::While,
        "for" => TokenKind::For,
        "in" => TokenKind::In,
        "break" => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "const" => TokenKind::Const,
        "static" => TokenKind::Static,
        "extern" => TokenKind::Extern,
        "crate" => TokenKind::Crate,
        "super" => TokenKind::Super,
        "dyn" => TokenKind::Dyn,
        "unsafe" => TokenKind::Unsafe,
        "as" => TokenKind::As,

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_lookup() {
        assert_eq!(keyword_from_str("fn"), Some(TokenKind::Fn));
        assert_eq!(keyword_from_str("theorem"), Some(TokenKind::Theorem));
        assert_eq!(keyword_from_str("async"), Some(TokenKind::Async));
        assert_eq!(keyword_from_str("notakeyword"), None);
    }

    #[test]
    fn test_token_kind_properties() {
        assert!(TokenKind::Whitespace.is_trivia());
        assert!(TokenKind::LineComment.is_trivia());
        assert!(!TokenKind::Fn.is_trivia());

        assert!(TokenKind::Fn.is_keyword());
        assert!(TokenKind::Theorem.is_keyword());
        assert!(!TokenKind::Async.is_keyword()); // reserved, not active

        assert!(TokenKind::Async.is_reserved());
        assert!(!TokenKind::Fn.is_reserved());

        assert!(TokenKind::Fn.can_start_item());
        assert!(TokenKind::Theorem.can_start_item());
        assert!(!TokenKind::Let.can_start_item());

        assert!(TokenKind::IntLiteral.can_start_expr());
        assert!(TokenKind::Ident.can_start_expr());
        assert!(!TokenKind::Comma.can_start_expr());
    }

    #[test]
    fn test_token_text() {
        let source = "fn foo";
        let token = Token::new(TokenKind::Fn, Span::new(0, 2));
        assert_eq!(token.text(source), "fn");
    }
}
