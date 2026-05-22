//! Surface AST for the Tungsten language.
//!
//! This module defines the surface syntax AST that the parser produces.
//! It will later be elaborated into core terms for type checking.

mod expr;
mod items;
mod path;

pub use expr::*;
pub use items::*;
pub use path::*;

use crate::span::{Span, Spanned};
use serde::{Deserialize, Serialize};

/// A complete source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    /// All items in the file
    pub items: Vec<Item>,
    /// Span covering the entire file
    pub span: Span,
}

impl Spanned for SourceFile {
    fn span(&self) -> Span {
        self.span
    }
}

/// A top-level item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Item {
    /// Function definition: `fn name(params) -> RetType { body }`
    Function(FunctionDef),
    /// Type definition: `type Name<params> = variants`
    TypeDef(TypeDef),
    /// Type alias: `type Name = OtherType`
    TypeAlias(TypeAlias),
    /// Theorem: `theorem name(params): prop { proof }`
    Theorem(TheoremDef),
    /// Lemma: `lemma name(params): prop { proof }`
    Lemma(TheoremDef),
    /// Axiom: `axiom name(params): prop`
    Axiom(AxiomDef),
    /// External function declaration: `extern fn name(params) -> RetType`
    ExternFn(ExternFnDef),
    /// Module declaration: `mod foo;`
    Mod(ModDecl),
    /// Use statement: `use foo::bar;`
    Use(UseDecl),
    /// Error recovery placeholder
    Error(Span),
}

impl Spanned for Item {
    fn span(&self) -> Span {
        match self {
            Item::Function(f) => f.span,
            Item::TypeDef(t) => t.span,
            Item::TypeAlias(t) => t.span,
            Item::Theorem(t) => t.span,
            Item::Lemma(t) => t.span,
            Item::Axiom(a) => a.span,
            Item::ExternFn(e) => e.span,
            Item::Mod(m) => m.span,
            Item::Use(u) => u.span,
            Item::Error(s) => *s,
        }
    }
}

/// Visibility modifier for items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Visibility {
    /// Private (default): no modifier
    /// Visible within declaring module and its submodules.
    #[default]
    Private,
    /// Crate-public: `pub(crate)`
    /// Visible anywhere within the current crate.
    Crate,
    /// Public: `pub`
    /// Visible to external crates/consumers.
    Public,
}

/// A type parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeParam {
    /// Parameter name
    pub name: Ident,
    /// Optional bounds (future: trait bounds)
    pub bounds: Vec<TypeExpr>,
    /// Span
    pub span: Span,
}

/// A function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    /// Parameter pattern (usually just a name)
    pub pattern: Pattern,
    /// Parameter type
    pub ty: TypeExpr,
    /// Span
    pub span: Span,
}

/// An identifier with source location.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ident {
    /// The identifier name
    pub name: String,
    /// Source span
    pub span: Span,
}

impl Ident {
    /// Create a new identifier.
    #[must_use]
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }
}

impl Spanned for Ident {
    fn span(&self) -> Span {
        self.span
    }
}

/// A type expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeExpr {
    /// Type name or path: `Nat`, `Bool`, `foo::Config`
    Path(Path),
    /// Function type: `A -> B`
    Arrow(Box<TypeExpr>, Box<TypeExpr>, Span),
    /// Product type: `A * B`
    Product(Box<TypeExpr>, Box<TypeExpr>, Span),
    /// Sum type: `A + B`
    Sum(Box<TypeExpr>, Box<TypeExpr>, Span),
    /// Generic application: `List<T>`, `Either<A, B>`
    App(Box<TypeExpr>, Vec<TypeExpr>, Span),
    /// Universal quantification: `forall T. A`
    Forall(Ident, Box<TypeExpr>, Span),
    /// Proposition type: `Prop`
    Prop(Span),
    /// Unit type: `Unit` or `()`
    Unit(Span),
    /// Void type: `Void` or `!`
    Void(Span),
    /// Pointer type: `*T` (for FFI)
    Ptr(Box<TypeExpr>, Span),
    /// Ref type: `Ref<T>` (mutable reference)
    Ref(Box<TypeExpr>, Span),
    /// Equality type: `a == b`
    Eq(Box<Expr>, Box<Expr>, Span),
    /// Explicit equality type: `Eq<T, a, b>`
    EqExplicit(Box<TypeExpr>, Box<Expr>, Box<Expr>, Span),
    /// Parenthesized: `(T)`
    Paren(Box<TypeExpr>, Span),
    /// Error placeholder
    Error(Span),
}

impl Spanned for TypeExpr {
    fn span(&self) -> Span {
        match self {
            TypeExpr::Path(p) => p.span,
            TypeExpr::Arrow(_, _, s) => *s,
            TypeExpr::Product(_, _, s) => *s,
            TypeExpr::Sum(_, _, s) => *s,
            TypeExpr::App(_, _, s) => *s,
            TypeExpr::Forall(_, _, s) => *s,
            TypeExpr::Prop(s) => *s,
            TypeExpr::Unit(s) => *s,
            TypeExpr::Void(s) => *s,
            TypeExpr::Ptr(_, s) => *s,
            TypeExpr::Ref(_, s) => *s,
            TypeExpr::Eq(_, _, s) => *s,
            TypeExpr::EqExplicit(_, _, _, s) => *s,
            TypeExpr::Paren(_, s) => *s,
            TypeExpr::Error(s) => *s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ident() {
        let id = Ident::new("foo", Span::new(0, 3));
        assert_eq!(id.name, "foo");
        assert_eq!(id.span(), Span::new(0, 3));
    }

    #[test]
    fn test_binop_precedence() {
        // Multiplication binds tighter than addition
        assert!(BinOp::Mul.precedence() > BinOp::Add.precedence());
        // Addition binds tighter than comparison
        assert!(BinOp::Add.precedence() > BinOp::Eq.precedence());
        // Comparison binds tighter than logical and
        assert!(BinOp::Eq.precedence() > BinOp::And.precedence());
    }

    #[test]
    fn test_sorry() {
        let sorry = Sorry::new(Span::new(0, 5));
        assert!(sorry.name.is_none());

        let sorry = Sorry::with_name(Span::new(0, 5), "TODO: prove this");
        assert_eq!(sorry.name, Some("TODO: prove this".to_string()));
    }
}
