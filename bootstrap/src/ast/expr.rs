//! Expression, pattern, and operator types for the Tungsten AST.

use serde::{Deserialize, Serialize};

use super::items::UseDecl;
use super::{Ident, Path, Span, Spanned, TypeExpr};

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
    /// Let-else: `let P = e1 else diverge; e2`
    LetElse(
        Pattern,
        Option<TypeExpr>,
        Box<Expr>,
        Box<Expr>,
        Box<Expr>,
        Span,
    ),
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
    /// Named record constructor: `Point { x: 10, y: 20 }` or `Point { ...p, x: 10 }` (ADR 13.5.26h/i)
    NamedRecord {
        /// Type name path (e.g., `Point`)
        name: Path,
        /// Optional spread expression (e.g., `...base`)
        spread: Option<Box<Expr>>,
        /// Field initializers
        fields: Vec<(Ident, Expr)>,
        /// Span of the entire expression
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
    /// Try: `expr?`
    Try(Box<Expr>, Span),
    /// Try block: `try { body }` (ADR 15.5.26d)
    TryBlock(Box<Expr>, Span),
    /// If-let: `if let P = e { body }` or `if let P = e { body } else { fallback }`
    IfLet(Pattern, Box<Expr>, Box<Expr>, Option<Box<Expr>>, Span),
    /// If-let chain: `if let P1 = e1 && let P2 = e2 && guard { body } [else { fallback }]` (ADR 15.5.26d)
    IfLetChain(Vec<IfLetCondition>, Box<Expr>, Option<Box<Expr>>, Span),
    /// Have expression (proof): `have h: P = proof; body`
    Have(Ident, TypeExpr, Box<Expr>, Box<Expr>, Span),
    /// Show expression (proof): `show P { proof }`
    Show(TypeExpr, Box<Expr>, Span),
    /// Assume (proof): `assume h: P; body`
    Assume(Ident, TypeExpr, Box<Expr>, Span),
    /// Reflexivity: `refl`
    Refl(Span),
    /// Substitution: `subst(proof, motive, witness)` (ADR 21.5.26g)
    Subst(Box<Expr>, Motive, Box<Expr>, Span),
    /// Symmetry: `sym(proof)`
    Sym(Box<Expr>, Span),
    /// Transitivity: `trans(h1, h2)`
    Trans(Box<Expr>, Box<Expr>, Span),
    /// Congruence: `cong(f, proof)`
    Cong(Box<Expr>, Box<Expr>, Span),
    /// Natural number induction: `natind(motive, base, step, n)` (ADR 22.5.26a)
    NatInd(Motive, Box<Expr>, Box<Expr>, Box<Expr>, Span),
    /// Natural number primitive recursion: `natrec(type, base, step, n)` (ADR 22.5.26a)
    NatRec(Box<TypeExpr>, Box<Expr>, Box<Expr>, Box<Expr>, Span),
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
            Expr::LetElse(_, _, _, _, _, s) => *s,
            Expr::If(_, _, _, s) => *s,
            Expr::Match(_, _, s) => *s,
            Expr::Block(_, _, s) => *s,
            Expr::Tuple(_, s) => *s,
            Expr::RecordLit { span, .. } => *span,
            Expr::NamedRecord { span, .. } => *span,
            Expr::Field(_, _, s) => *s,
            Expr::TypeApp(_, _, s) => *s,
            Expr::Annot(_, _, s) => *s,
            Expr::Return(_, s) => *s,
            Expr::Try(_, s) => *s,
            Expr::TryBlock(_, s) => *s,
            Expr::IfLet(_, _, _, _, s) => *s,
            Expr::IfLetChain(_, _, _, s) => *s,
            Expr::Have(_, _, _, _, s) => *s,
            Expr::Show(_, _, s) => *s,
            Expr::Assume(_, _, _, s) => *s,
            Expr::Refl(s) => *s,
            Expr::Subst(_, _, _, s) => *s,
            Expr::Sym(_, s) => *s,
            Expr::Trans(_, _, s) => *s,
            Expr::Cong(_, _, s) => *s,
            Expr::NatInd(_, _, _, _, s) => *s,
            Expr::NatRec(_, _, _, _, s) => *s,
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

/// A motive in `subst(proof, motive, witness)` (ADR 21.5.26g).
///
/// Motives are type-level predicates of the form `|x: τ| <type-body>`.
/// The body is elaborated as a type expression, not a term.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Motive {
    /// Typed predicate lambda: `|x: τ| <type-body>`
    Lambda(LambdaParam, Box<TypeExpr>, Span),
    /// Raw expression (will be rejected by elaborator with MotiveNotPredicate)
    Expr(Box<Expr>),
}

impl Spanned for Motive {
    fn span(&self) -> Span {
        match self {
            Motive::Lambda(_, _, span) => *span,
            Motive::Expr(expr) => expr.span(),
        }
    }
}

/// A condition in an `if let` chain (ADR 15.5.26d).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IfLetCondition {
    /// Pattern binding: `let P = expr`
    Bind(Pattern, Box<Expr>),
    /// Boolean guard: `expr`
    Guard(Box<Expr>),
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
    /// Let-else statement: `let P = e else diverge;`
    LetElse(Pattern, Option<TypeExpr>, Expr, Expr, Span),
    /// Expression statement: `e;`
    Expr(Expr, Span),
    /// Item definition (nested function, etc.)
    Item(super::Item),
}

impl Spanned for Stmt {
    fn span(&self) -> Span {
        match self {
            Stmt::Let(_, _, _, s) => *s,
            Stmt::LetElse(_, _, _, _, s) => *s,
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
