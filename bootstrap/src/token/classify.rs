//! TokenKind classification methods.
//!
//! Predicates for checking token categories: trivia, keywords,
//! reserved words, literals, expression/item starters.

use super::TokenKind;

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
                | TokenKind::Subst
                | TokenKind::Sym
                | TokenKind::Trans
                | TokenKind::Cong
                | TokenKind::NatInd
                | TokenKind::NatRec
                | TokenKind::Bool
                | TokenKind::Nat
                | TokenKind::Unit
                | TokenKind::Void
                | TokenKind::Extern
                | TokenKind::Ref
                | TokenKind::Pub
                | TokenKind::TryBlock
                | TokenKind::As
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
                | TokenKind::TryBlock
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
}
