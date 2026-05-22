//! Term analysis
//!
//! Functions for analyzing term structure: free variables, type variables, and value checking.

mod spans;
mod use_count;

use std::collections::HashSet;

use crate::types::{TyVar, Type};

use super::{Term, Var};

/// Collect free term variables from multiple sub-terms.
fn collect_fv(terms: &[&Box<Term>]) -> HashSet<Var> {
    let mut set = HashSet::new();
    for t in terms {
        set.extend(t.free_vars());
    }
    set
}

/// Collect free type variables from type annotations and sub-terms.
fn collect_ftv(types: &[&Type], terms: &[&Box<Term>]) -> HashSet<TyVar> {
    let mut set = HashSet::new();
    for ty in types {
        set.extend(ty.free_type_vars());
    }
    for t in terms {
        set.extend(t.free_type_vars());
    }
    set
}

impl Term {
    /// Get free term variables
    #[must_use]
    pub fn free_vars(&self) -> HashSet<Var> {
        match self {
            Term::Var(v) => {
                let mut set = HashSet::new();
                set.insert(v.clone());
                set
            }

            // Leaf terms: no free variables
            Term::Global(_)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::Zero
            | Term::NatLit(_)
            | Term::Sorry
            | Term::StringLit(_) => HashSet::new(),

            // Binding forms: recurse then remove bound variable
            Term::Lambda(x, _, body) | Term::Fix(x, _, body) => {
                let mut set = body.free_vars();
                set.remove(x);
                set
            }
            Term::Let(x, _, def, body) => {
                let mut set = def.free_vars();
                let mut body_free = body.free_vars();
                body_free.remove(x);
                set.extend(body_free);
                set
            }

            // Unary: free vars of single sub-term
            Term::Absurd(_, t)
            | Term::Succ(t)
            | Term::Fst(t)
            | Term::Snd(t)
            | Term::Inl(_, t)
            | Term::Inr(_, t)
            | Term::TyAbs(_, t)
            | Term::TyApp(t, _)
            | Term::Refl(_, t)
            | Term::Annot(t, _)
            | Term::StrLen(t)
            | Term::Fold(_, t)
            | Term::Unfold(_, t)
            | Term::BoolNot(t)
            | Term::RefNew(t)
            | Term::RefGet(t)
            | Term::Return(t)
            | Term::Spanned(t, _) => t.free_vars(),

            // Binary: union free vars from both sub-terms
            Term::App(t1, t2)
            | Term::Pair(t1, t2)
            | Term::StrConcat(t1, t2)
            | Term::StrEq(t1, t2)
            | Term::NatAdd(t1, t2)
            | Term::NatSub(t1, t2)
            | Term::NatMul(t1, t2)
            | Term::NatDiv(t1, t2)
            | Term::NatMod(t1, t2)
            | Term::NatEq(t1, t2)
            | Term::BoolAnd(t1, t2)
            | Term::BoolOr(t1, t2)
            | Term::NatLt(t1, t2)
            | Term::NatLe(t1, t2)
            | Term::NatGt(t1, t2)
            | Term::NatGe(t1, t2)
            | Term::StrCharAt(t1, t2)
            | Term::RefSet(t1, t2) => collect_fv(&[t1, t2]),

            Term::If(t1, t2, t3) | Term::StrSubstring(t1, t2, t3) => collect_fv(&[t1, t2, t3]),

            Term::NatRec(_, z, s, n) | Term::NatInd(_, z, s, n) => collect_fv(&[z, s, n]),
            Term::Subst(_, _, eq, proof) => collect_fv(&[eq, proof]),

            Term::Case(scrut, x, t1, y, t2) => {
                let mut set = scrut.free_vars();
                let mut t1_free = t1.free_vars();
                t1_free.remove(x);
                let mut t2_free = t2.free_vars();
                t2_free.remove(y);
                set.extend(t1_free);
                set.extend(t2_free);
                set
            }

            Term::ExternCall(_, args) => {
                let mut set = HashSet::new();
                for arg in args {
                    set.extend(arg.free_vars());
                }
                set
            }

            Term::AdtConstruct(_, _, payload) => payload.free_vars(),

            Term::AdtMatch(scrut, arms) => {
                let mut set = scrut.free_vars();
                for (_, var, body) in arms {
                    let mut arm_free = body.free_vars();
                    arm_free.remove(var);
                    set.extend(arm_free);
                }
                set
            }
        }
    }

    /// Get free type variables
    #[must_use]
    pub fn free_type_vars(&self) -> HashSet<TyVar> {
        match self {
            // Leaf terms: no type variables
            Term::Var(_)
            | Term::Global(_)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::Zero
            | Term::NatLit(_)
            | Term::Sorry
            | Term::StringLit(_) => HashSet::new(),

            // Binding: string + type + sub-term
            Term::Lambda(_, ty, body) | Term::Fix(_, ty, body) => collect_ftv(&[ty], &[body]),

            // Type + sub-term (type first)
            Term::Absurd(ty, t)
            | Term::Refl(ty, t)
            | Term::Inl(ty, t)
            | Term::Inr(ty, t)
            | Term::Fold(ty, t)
            | Term::Unfold(ty, t) => collect_ftv(&[ty], &[t]),

            // Sub-term + type (type second)
            Term::TyApp(t, ty) | Term::Annot(t, ty) => collect_ftv(&[ty], &[t]),

            // Unary: sub-term only (no type annotations in variant)
            Term::Succ(t)
            | Term::Fst(t)
            | Term::Snd(t)
            | Term::StrLen(t)
            | Term::BoolNot(t)
            | Term::RefNew(t)
            | Term::RefGet(t)
            | Term::Return(t)
            | Term::Spanned(t, _) => t.free_type_vars(),

            // Binary: two sub-terms only
            Term::App(t1, t2)
            | Term::Pair(t1, t2)
            | Term::StrConcat(t1, t2)
            | Term::StrEq(t1, t2)
            | Term::NatAdd(t1, t2)
            | Term::NatSub(t1, t2)
            | Term::NatMul(t1, t2)
            | Term::NatDiv(t1, t2)
            | Term::NatMod(t1, t2)
            | Term::NatEq(t1, t2)
            | Term::BoolAnd(t1, t2)
            | Term::BoolOr(t1, t2)
            | Term::NatLt(t1, t2)
            | Term::NatLe(t1, t2)
            | Term::NatGt(t1, t2)
            | Term::NatGe(t1, t2)
            | Term::StrCharAt(t1, t2)
            | Term::RefSet(t1, t2) => collect_ftv(&[], &[t1, t2]),

            Term::If(t1, t2, t3) | Term::StrSubstring(t1, t2, t3) => {
                collect_ftv(&[], &[t1, t2, t3])
            }

            Term::Let(_, ty, def, body) => collect_ftv(&[ty], &[def, body]),
            Term::NatRec(ty, z, s, n) | Term::NatInd(ty, z, s, n) => collect_ftv(&[ty], &[z, s, n]),
            Term::Case(scrut, _, t1, _, t2) => collect_ftv(&[], &[scrut, t1, t2]),
            Term::Subst(ty, p, eq, proof) => collect_ftv(&[ty, p], &[eq, proof]),

            Term::TyAbs(alpha, body) => {
                let mut set = body.free_type_vars();
                set.remove(alpha);
                set
            }

            Term::ExternCall(_, args) => {
                let mut set = HashSet::new();
                for arg in args {
                    set.extend(arg.free_type_vars());
                }
                set
            }

            Term::AdtConstruct(adt_ty, _, payload) => collect_ftv(&[adt_ty], &[payload]),

            Term::AdtMatch(scrut, arms) => {
                let mut set = scrut.free_type_vars();
                for (_, _, body) in arms {
                    set.extend(body.free_type_vars());
                }
                set
            }
        }
    }

    /// Check if term is a value (canonical form)
    #[must_use]
    pub fn is_value(&self) -> bool {
        match self {
            Term::Lambda(_, _, _) => true,
            Term::True | Term::False => true,
            Term::Zero | Term::NatLit(_) => true,
            Term::Succ(t) => t.is_value(),
            Term::Unit => true,
            Term::Pair(t1, t2) => t1.is_value() && t2.is_value(),
            Term::Inl(_, t) | Term::Inr(_, t) => t.is_value(),
            Term::TyAbs(_, _) => true,
            Term::Refl(_, t) => t.is_value(),
            // Phase 2A
            Term::StringLit(_) => true,
            Term::Fold(_, t) => t.is_value(),
            // Phase 2B: Flat ADT - constructed ADT is a value if payload is
            Term::AdtConstruct(_, _, payload) => payload.is_value(),
            Term::Spanned(inner, _) => inner.is_value(),
            // Phase 3-Prep: RefNew creates a new ref cell (value once allocated)
            // ExternCall results are values once returned
            _ => false,
        }
    }
}
