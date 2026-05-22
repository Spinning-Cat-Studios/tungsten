//! Term constructors
//!
//! Core constructors for variables, bindings, type-level operations, proofs,
//! and boolean operations.

use crate::types::Type;

use super::Term;

impl Term {
    // === Core Constructors ===

    /// Create a variable reference
    pub fn var(name: impl Into<String>) -> Term {
        Term::Var(name.into())
    }

    /// Create a global reference (top-level definition, resolved at runtime)
    pub fn global(name: impl Into<String>) -> Term {
        Term::Global(name.into())
    }

    /// Create a lambda abstraction λx:τ. t
    pub fn lambda(var: impl Into<String>, ty: Type, body: Term) -> Term {
        Term::Lambda(var.into(), ty, Box::new(body))
    }

    /// Create an application t₁ t₂
    #[must_use]
    pub fn app(t1: Term, t2: Term) -> Term {
        Term::App(Box::new(t1), Box::new(t2))
    }

    /// Create a let binding
    pub fn let_in(var: impl Into<String>, ty: Type, def: Term, body: Term) -> Term {
        Term::Let(var.into(), ty, Box::new(def), Box::new(body))
    }

    /// Wrap a term with a source span for debug locations (ADR 17.4.26a §3.1).
    #[must_use]
    pub fn spanned(term: Term, span: super::TermSpan) -> Term {
        Term::Spanned(Box::new(term), span)
    }

    // === Type-level constructors ===

    /// Create type abstraction Λα. t
    pub fn ty_abs(var: impl Into<String>, body: Term) -> Term {
        Term::TyAbs(var.into(), Box::new(body))
    }

    /// Create type application t [τ]
    #[must_use]
    pub fn ty_app(t: Term, ty: Type) -> Term {
        Term::TyApp(Box::new(t), ty)
    }

    // === Proof constructors ===

    /// Create reflexivity proof refl [τ] t
    #[must_use]
    pub fn refl(ty: Type, t: Term) -> Term {
        Term::Refl(ty, Box::new(t))
    }

    /// Create substitution subst [τ] [P] `t_eq` `t_proof`
    #[must_use]
    pub fn subst(ty: Type, motive: Type, eq_proof: Term, proof: Term) -> Term {
        Term::Subst(ty, motive, Box::new(eq_proof), Box::new(proof))
    }

    /// Create type annotation (t : τ)
    #[must_use]
    pub fn annot(t: Term, ty: Type) -> Term {
        Term::Annot(Box::new(t), ty)
    }

    // === Control Flow ===

    /// Create early return: return e
    #[must_use]
    pub fn early_return(t: Term) -> Term {
        Term::Return(Box::new(t))
    }

    // === Boolean Operations ===

    /// Create boolean and: a && b
    #[must_use]
    pub fn bool_and(a: Term, b: Term) -> Term {
        Term::BoolAnd(Box::new(a), Box::new(b))
    }

    /// Create boolean or: a || b
    #[must_use]
    pub fn bool_or(a: Term, b: Term) -> Term {
        Term::BoolOr(Box::new(a), Box::new(b))
    }

    /// Create boolean not: !a
    #[must_use]
    pub fn bool_not(a: Term) -> Term {
        Term::BoolNot(Box::new(a))
    }
}
