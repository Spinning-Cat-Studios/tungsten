//! Token types for the Tungsten lexer.
//!
//! Defines the ~50 token kinds used by the lexer and parser.

mod classify;
mod display;

pub use display::keyword_from_str;

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
    /// `try`
    TryBlock,
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
    /// `subst` - substitution/transport
    Subst,
    /// `sym` - symmetry of equality
    Sym,
    /// `trans` - transitivity of equality
    Trans,
    /// `cong` - congruence of equality
    Cong,
    /// `natind` - natural number induction
    NatInd,
    /// `natrec` - natural number primitive recursion
    NatRec,

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

/// A token with its kind and source span.
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
