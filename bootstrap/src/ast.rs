//! Surface AST for the Tungsten language.
//!
//! This module defines the surface syntax AST that the parser produces.
//! It will later be elaborated into core terms for type checking.

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

/// A function definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Function name
    pub name: Ident,
    /// Type parameters (generics)
    pub type_params: Vec<TypeParam>,
    /// Parameters
    pub params: Vec<Param>,
    /// Return type (optional, can be inferred)
    pub return_type: Option<TypeExpr>,
    /// Function body
    pub body: Expr,
    /// Span of the entire definition
    pub span: Span,
}

/// A type definition (ADT or Record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Type name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// Type body (sum type with variants, or record type with fields)
    pub body: TypeBody,
    /// Span of the entire definition
    pub span: Span,
}

/// The body of a type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeBody {
    /// Sum type with variants: `| A | B(T) | C(T, U)`
    Sum(Vec<Variant>),
    /// Record type with named fields: `{ x: T, y: U }`
    Record(Vec<RecordField>),
}

/// A field in a record type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordField {
    /// Field name
    pub name: Ident,
    /// Field type
    pub ty: TypeExpr,
    /// Span of the field
    pub span: Span,
}

/// A type alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAlias {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Alias name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// The aliased type
    pub ty: TypeExpr,
    /// Span of the entire definition
    pub span: Span,
}

/// A variant of an ADT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    /// Variant name
    pub name: Ident,
    /// Fields (empty for unit variants)
    pub fields: Vec<Field>,
    /// Span of the variant
    pub span: Span,
}

/// A field in a variant or struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    /// Optional field name (for named fields)
    pub name: Option<Ident>,
    /// Field type
    pub ty: TypeExpr,
    /// Span of the field
    pub span: Span,
}

/// A theorem definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TheoremDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Theorem name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// Parameters (hypotheses)
    pub params: Vec<Param>,
    /// Proposition to prove
    pub prop: TypeExpr,
    /// Proof body
    pub body: Expr,
    /// Span of the entire definition
    pub span: Span,
}

/// An axiom definition (theorem without proof).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxiomDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Axiom name
    pub name: Ident,
    /// Type parameters
    pub type_params: Vec<TypeParam>,
    /// Parameters
    pub params: Vec<Param>,
    /// Proposition
    pub prop: TypeExpr,
    /// Span of the entire definition
    pub span: Span,
}

/// An external function definition (FFI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternFnDef {
    /// Visibility modifier
    pub visibility: Visibility,
    /// Function name (Tungsten side)
    pub name: Ident,
    /// Optional C symbol name (if different from Tungsten name)
    pub symbol: Option<String>,
    /// ABI string (e.g., "C")
    pub abi: String,
    /// Parameters with types
    pub params: Vec<ExternParam>,
    /// Return type
    pub return_type: TypeExpr,
    /// Span of the entire definition
    pub span: Span,
}

/// Visibility modifier for items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// Private (default): no modifier
    /// Visible within declaring module and its submodules.
    Private,
    /// Crate-public: `pub(crate)`
    /// Visible anywhere within the current crate.
    Crate,
    /// Public: `pub`
    /// Visible to external crates/consumers.
    Public,
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Private
    }
}

/// A module declaration: `mod foo;` or `pub mod foo;`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDecl {
    /// Visibility (public or private)
    pub visibility: Visibility,
    /// Module name (e.g., "token" for `mod token;`)
    pub name: Ident,
    /// Span of the entire declaration
    pub span: Span,
}

/// A use statement: `use foo::bar;` or `use foo::{a, b};`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseDecl {
    /// Visibility (for future `pub use`)
    pub visibility: Visibility,
    /// The use tree (path and optional grouping)
    pub tree: UseTree,
    /// Span of the entire declaration
    pub span: Span,
}

/// The tree structure of a use statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UseTree {
    /// Simple path: `use foo::bar;`
    Path(Path),
    /// Grouped imports: `use foo::{bar, baz};`
    Group {
        /// Prefix path (e.g., `foo` in `use foo::{bar, baz};`)
        prefix: Path,
        /// Items being imported
        items: Vec<UseTree>,
        /// Span of the group
        span: Span,
    },
    /// Glob import: `use foo::*;`
    Glob {
        /// Module path to import from (e.g., `foo` in `use foo::*;`)
        prefix: Path,
        /// Span of the glob (including the `*`)
        span: Span,
    },
    // Future: Alias(`use foo::bar as baz`)
}

/// Result of expanding a use tree - either concrete paths or a glob import.
#[derive(Debug, Clone)]
pub enum ExpandedUseTree {
    /// Concrete paths that can be individually imported
    Paths(Vec<Path>),
    /// Glob import that needs special handling
    Glob { prefix: Path, span: Span },
}

impl UseTree {
    /// Expand this tree into paths or glob imports.
    ///
    /// Grouped imports are flattened into individual paths.
    /// Glob imports cannot be statically expanded and are returned as-is.
    pub fn expand(&self) -> ExpandedUseTree {
        match self {
            UseTree::Path(path) => ExpandedUseTree::Paths(vec![path.clone()]),
            UseTree::Group { prefix, items, .. } => {
                let mut all_paths = Vec::new();
                for item in items {
                    match item.expand() {
                        ExpandedUseTree::Paths(paths) => {
                            for p in paths {
                                // Prepend prefix to each expanded path
                                let mut full_segments = prefix.segments.clone();
                                full_segments.extend(p.segments);
                                let span = Span::new(prefix.span.start, p.span.end);
                                all_paths.push(Path {
                                    segments: full_segments,
                                    span,
                                });
                            }
                        }
                        ExpandedUseTree::Glob {
                            prefix: glob_prefix,
                            span,
                        } => {
                            // Glob inside a group: prepend the group's prefix
                            let mut full_segments = prefix.segments.clone();
                            full_segments.extend(glob_prefix.segments);
                            let full_prefix = Path {
                                segments: full_segments,
                                span: Span::new(prefix.span.start, glob_prefix.span.end),
                            };
                            // Return early - can't mix globs with regular paths in expansion
                            return ExpandedUseTree::Glob {
                                prefix: full_prefix,
                                span,
                            };
                        }
                    }
                }
                ExpandedUseTree::Paths(all_paths)
            }
            UseTree::Glob { prefix, span } => ExpandedUseTree::Glob {
                prefix: prefix.clone(),
                span: *span,
            },
        }
    }
}

/// An external function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternParam {
    /// Parameter name
    pub name: Ident,
    /// Parameter type
    pub ty: TypeExpr,
    /// Span
    pub span: Span,
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

/// A path with optional module segments: `foo::bar::baz`
///
/// Used for qualified names in expressions, types, and patterns.
/// A single-segment path is equivalent to an unqualified identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Path {
    /// Path segments (e.g., ["foo", "bar", "baz"])
    pub segments: Vec<Ident>,
    /// Span covering the entire path
    pub span: Span,
}

impl Path {
    /// Create a simple single-segment path (an unqualified name).
    #[must_use]
    pub fn simple(name: Ident) -> Self {
        let span = name.span;
        Self {
            segments: vec![name],
            span,
        }
    }

    /// Check if this is a simple unqualified name (single segment).
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    /// Get the final segment (item name).
    ///
    /// # Panics
    /// Panics if the path has no segments (should never happen for valid paths).
    #[must_use]
    pub fn item_name(&self) -> &Ident {
        self.segments
            .last()
            .expect("path must have at least one segment")
    }

    /// Get module segments (all but the last).
    #[must_use]
    pub fn module_segments(&self) -> &[Ident] {
        if self.segments.is_empty() {
            &[]
        } else {
            &self.segments[..self.segments.len() - 1]
        }
    }
}

impl Spanned for Path {
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
            TypeExpr::Paren(_, s) => *s,
            TypeExpr::Error(s) => *s,
        }
    }
}

/// An expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// Variable or path reference: `x`, `foo::bar`
    Path(Path),
    /// Integer literal: `42`
    IntLiteral(u64, Span),
    /// Boolean literal: `true`, `false`
    BoolLiteral(bool, Span),
    /// String literal: `"hello"`
    StringLiteral(String, Span),
    /// Unit value: `()`
    Unit(Span),
    /// Lambda: `|x| body` or `fn(x: T) => body`
    Lambda(Vec<LambdaParam>, Box<Expr>, Span),
    /// Function application: `f(x, y)`
    App(Box<Expr>, Vec<Expr>, Span),
    /// Binary operation: `a + b`
    Binary(Box<Expr>, BinOp, Box<Expr>, Span),
    /// Unary operation: `!x`, `-x`
    Unary(UnaryOp, Box<Expr>, Span),
    /// Let binding: `let x = e1; e2` or `let x: T = e1; e2`
    Let(Pattern, Option<TypeExpr>, Box<Expr>, Box<Expr>, Span),
    /// If expression: `if cond { then } else { else }`
    If(Box<Expr>, Box<Expr>, Box<Expr>, Span),
    /// Match expression: `match e { arms }`
    Match(Box<Expr>, Vec<MatchArm>, Span),
    /// Block: `{ stmts; expr }`
    Block(Vec<Stmt>, Option<Box<Expr>>, Span),
    /// Tuple: `(a, b, c)`
    Tuple(Vec<Expr>, Span),
    /// Record literal: `{ x: 10, y: 20 }` or `{ ...base, x: 10 }`
    RecordLit {
        /// Optional spread expression (e.g., `...base`)
        spread: Option<Box<Expr>>,
        /// Explicit fields
        fields: Vec<(Ident, Expr)>,
        /// Span of the entire literal
        span: Span,
    },
    /// Field access: `e.field`
    Field(Box<Expr>, Ident, Span),
    /// Type application: `f::<T>`
    TypeApp(Box<Expr>, Vec<TypeExpr>, Span),
    /// Type annotation: `e : T`
    Annot(Box<Expr>, TypeExpr, Span),
    /// Return: `return e`
    Return(Option<Box<Expr>>, Span),
    /// Have expression (proof): `have h: P = proof; body`
    Have(Ident, TypeExpr, Box<Expr>, Box<Expr>, Span),
    /// Show expression (proof): `show P { proof }`
    Show(TypeExpr, Box<Expr>, Span),
    /// Assume (proof): `assume h: P; body`
    Assume(Ident, TypeExpr, Box<Expr>, Span),
    /// Reflexivity: `refl`
    Refl(Span),
    /// Sorry: incomplete proof placeholder
    Sorry(Sorry),
    /// Parenthesized: `(e)`
    Paren(Box<Expr>, Span),
    /// Error placeholder
    Error(Span),
}

impl Spanned for Expr {
    fn span(&self) -> Span {
        match self {
            Expr::Path(p) => p.span,
            Expr::IntLiteral(_, s) => *s,
            Expr::BoolLiteral(_, s) => *s,
            Expr::StringLiteral(_, s) => *s,
            Expr::Unit(s) => *s,
            Expr::Lambda(_, _, s) => *s,
            Expr::App(_, _, s) => *s,
            Expr::Binary(_, _, _, s) => *s,
            Expr::Unary(_, _, s) => *s,
            Expr::Let(_, _, _, _, s) => *s,
            Expr::If(_, _, _, s) => *s,
            Expr::Match(_, _, s) => *s,
            Expr::Block(_, _, s) => *s,
            Expr::Tuple(_, s) => *s,
            Expr::RecordLit { span, .. } => *span,
            Expr::Field(_, _, s) => *s,
            Expr::TypeApp(_, _, s) => *s,
            Expr::Annot(_, _, s) => *s,
            Expr::Return(_, s) => *s,
            Expr::Have(_, _, _, _, s) => *s,
            Expr::Show(_, _, s) => *s,
            Expr::Assume(_, _, _, s) => *s,
            Expr::Refl(s) => *s,
            Expr::Sorry(sorry) => sorry.span,
            Expr::Paren(_, s) => *s,
            Expr::Error(s) => *s,
        }
    }
}

/// Lambda parameter (can have optional type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaParam {
    /// Parameter pattern
    pub pattern: Pattern,
    /// Optional type annotation
    pub ty: Option<TypeExpr>,
    /// Span
    pub span: Span,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical
    And,
    Or,
    // Function composition/application
    Pipe, // |>
    // String
    Concat, // ++
}

impl BinOp {
    /// Get the precedence of this operator (higher = binds tighter).
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne => 3,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::Pipe => 5,
            BinOp::Add | BinOp::Sub | BinOp::Concat => 6,
            BinOp::Mul | BinOp::Div | BinOp::Mod => 7,
        }
    }

    /// Check if this operator is right-associative.
    #[must_use]
    pub const fn is_right_assoc(self) -> bool {
        matches!(self, BinOp::Pipe)
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    /// Logical negation: `!`
    Not,
    /// Arithmetic negation: `-`
    Neg,
}

/// A pattern for matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pattern {
    /// Wildcard: `_`
    Wildcard(Span),
    /// Variable binding: `x`
    Var(Ident),
    /// Literal: `42`, `true`
    Literal(LiteralPattern),
    /// Tuple: `(a, b)`
    Tuple(Vec<Pattern>, Span),
    /// Constructor: `Some(x)`, `None`, `Result::Ok(v)`
    Constructor(Path, Vec<Pattern>, Span),
    /// Or pattern: `A | B`
    Or(Box<Pattern>, Box<Pattern>, Span),
    /// Error placeholder
    Error(Span),
}

impl Spanned for Pattern {
    fn span(&self) -> Span {
        match self {
            Pattern::Wildcard(s) => *s,
            Pattern::Var(id) => id.span,
            Pattern::Literal(lit) => lit.span(),
            Pattern::Tuple(_, s) => *s,
            Pattern::Constructor(_, _, s) => *s,
            Pattern::Or(_, _, s) => *s,
            Pattern::Error(s) => *s,
        }
    }
}

/// A literal pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LiteralPattern {
    Int(u64, Span),
    Bool(bool, Span),
    String(String, Span),
}

impl Spanned for LiteralPattern {
    fn span(&self) -> Span {
        match self {
            LiteralPattern::Int(_, s) => *s,
            LiteralPattern::Bool(_, s) => *s,
            LiteralPattern::String(_, s) => *s,
        }
    }
}

/// A match arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    /// Pattern to match
    pub pattern: Pattern,
    /// Optional guard: `if cond`
    pub guard: Option<Expr>,
    /// Body expression
    pub body: Expr,
    /// Span of the entire arm
    pub span: Span,
}

/// A statement in a block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    /// Let statement: `let x = e;`
    Let(Pattern, Option<TypeExpr>, Expr, Span),
    /// Expression statement: `e;`
    Expr(Expr, Span),
    /// Item definition (nested function, etc.)
    Item(Item),
}

impl Spanned for Stmt {
    fn span(&self) -> Span {
        match self {
            Stmt::Let(_, _, _, s) => *s,
            Stmt::Expr(_, s) => *s,
            Stmt::Item(item) => item.span(),
        }
    }
}

/// A sorry placeholder for incomplete proofs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sorry {
    /// Span of the sorry keyword
    pub span: Span,
    /// Optional name for tracking (for IDE/error reporting)
    pub name: Option<String>,
}

impl Sorry {
    /// Create a new sorry.
    #[must_use]
    pub fn new(span: Span) -> Self {
        Self { span, name: None }
    }

    /// Create a sorry with a tracking name.
    #[must_use]
    pub fn with_name(span: Span, name: impl Into<String>) -> Self {
        Self {
            span,
            name: Some(name.into()),
        }
    }
}

impl Spanned for Sorry {
    fn span(&self) -> Span {
        self.span
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
