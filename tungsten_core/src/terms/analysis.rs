//! Term analysis
//!
//! Functions for analyzing term structure: free variables, type variables, and value checking.

use std::collections::HashSet;

use crate::types::TyVar;

use super::{Term, Var};

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

            // Global references are defined names, not free variables
            Term::Global(_) => HashSet::new(),

            Term::Lambda(x, _, body) => {
                let mut set = body.free_vars();
                set.remove(x);
                set
            }

            Term::App(t1, t2) | Term::Pair(t1, t2) => {
                let mut set = t1.free_vars();
                set.extend(t2.free_vars());
                set
            }

            Term::Let(x, _, def, body) => {
                let mut set = def.free_vars();
                let mut body_free = body.free_vars();
                body_free.remove(x);
                set.extend(body_free);
                set
            }

            Term::True | Term::False | Term::Unit | Term::Zero | Term::NatLit(_) | Term::Sorry => {
                HashSet::new()
            }

            Term::If(c, t, e) => {
                let mut set = c.free_vars();
                set.extend(t.free_vars());
                set.extend(e.free_vars());
                set
            }

            Term::Absurd(_, t)
            | Term::Succ(t)
            | Term::Fst(t)
            | Term::Snd(t)
            | Term::Inl(_, t)
            | Term::Inr(_, t)
            | Term::TyAbs(_, t)
            | Term::TyApp(t, _)
            | Term::Refl(_, t)
            | Term::Annot(t, _) => t.free_vars(),

            Term::NatRec(_, z, s, n) | Term::NatInd(_, z, s, n) => {
                let mut set = z.free_vars();
                set.extend(s.free_vars());
                set.extend(n.free_vars());
                set
            }

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

            Term::Subst(_, _, eq, proof) => {
                let mut set = eq.free_vars();
                set.extend(proof.free_vars());
                set
            }

            // Phase 2A terms
            Term::StringLit(_) => HashSet::new(),
            Term::StrConcat(t1, t2) | Term::StrEq(t1, t2) => {
                let mut set = t1.free_vars();
                set.extend(t2.free_vars());
                set
            }
            Term::StrLen(t) | Term::Fold(_, t) | Term::Unfold(_, t) => t.free_vars(),
            Term::Fix(f, _, body) => {
                let mut set = body.free_vars();
                set.remove(f);
                set
            }

            // Phase 3C: Arithmetic
            Term::NatAdd(t1, t2)
            | Term::NatSub(t1, t2)
            | Term::NatMul(t1, t2)
            | Term::NatDiv(t1, t2)
            | Term::NatMod(t1, t2)
            | Term::NatEq(t1, t2)
            | Term::BoolAnd(t1, t2)
            | Term::BoolOr(t1, t2) => {
                let mut set = t1.free_vars();
                set.extend(t2.free_vars());
                set
            }
            Term::BoolNot(t) => t.free_vars(),

            // Phase 3-Prep terms
            Term::NatLt(t1, t2)
            | Term::NatLe(t1, t2)
            | Term::NatGt(t1, t2)
            | Term::NatGe(t1, t2) => {
                let mut set = t1.free_vars();
                set.extend(t2.free_vars());
                set
            }
            Term::StrCharAt(s, idx) => {
                let mut set = s.free_vars();
                set.extend(idx.free_vars());
                set
            }
            Term::StrSubstring(s, start, len) => {
                let mut set = s.free_vars();
                set.extend(start.free_vars());
                set.extend(len.free_vars());
                set
            }
            Term::ExternCall(_, args) => {
                let mut set = HashSet::new();
                for arg in args {
                    set.extend(arg.free_vars());
                }
                set
            }
            Term::RefNew(t) | Term::RefGet(t) => t.free_vars(),
            Term::RefSet(r, v) => {
                let mut set = r.free_vars();
                set.extend(v.free_vars());
                set
            }

            // Phase 2B: Flat ADT
            Term::AdtConstruct(_, _, payload) => payload.free_vars(),
            Term::AdtMatch(scrut, arms) => {
                let mut set = scrut.free_vars();
                for (_, var, body) in arms {
                    let mut arm_free = body.free_vars();
                    arm_free.remove(var); // var is bound in this arm
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
            Term::Var(_)
            | Term::Global(_)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::Zero
            | Term::NatLit(_)
            | Term::Sorry => HashSet::new(),

            Term::Lambda(_, ty, body) => {
                let mut set = ty.free_type_vars();
                set.extend(body.free_type_vars());
                set
            }

            Term::App(t1, t2) | Term::Pair(t1, t2) => {
                let mut set = t1.free_type_vars();
                set.extend(t2.free_type_vars());
                set
            }

            Term::Let(_, ty, def, body) => {
                let mut set = ty.free_type_vars();
                set.extend(def.free_type_vars());
                set.extend(body.free_type_vars());
                set
            }

            Term::If(c, t, e) => {
                let mut set = c.free_type_vars();
                set.extend(t.free_type_vars());
                set.extend(e.free_type_vars());
                set
            }

            Term::Absurd(ty, t) => {
                let mut set = ty.free_type_vars();
                set.extend(t.free_type_vars());
                set
            }

            Term::Succ(t) | Term::Fst(t) | Term::Snd(t) | Term::Refl(_, t) | Term::Annot(t, _) => {
                t.free_type_vars()
            }

            Term::NatRec(ty, z, s, n) => {
                let mut set = ty.free_type_vars();
                set.extend(z.free_type_vars());
                set.extend(s.free_type_vars());
                set.extend(n.free_type_vars());
                set
            }

            Term::NatInd(p, z, s, n) => {
                let mut set = p.free_type_vars();
                set.extend(z.free_type_vars());
                set.extend(s.free_type_vars());
                set.extend(n.free_type_vars());
                set
            }

            Term::Inl(ty, t) | Term::Inr(ty, t) => {
                let mut set = ty.free_type_vars();
                set.extend(t.free_type_vars());
                set
            }

            Term::Case(scrut, _, t1, _, t2) => {
                let mut set = scrut.free_type_vars();
                set.extend(t1.free_type_vars());
                set.extend(t2.free_type_vars());
                set
            }

            Term::TyAbs(alpha, body) => {
                let mut set = body.free_type_vars();
                set.remove(alpha);
                set
            }

            Term::TyApp(t, ty) => {
                let mut set = t.free_type_vars();
                set.extend(ty.free_type_vars());
                set
            }

            Term::Subst(ty, p, eq, proof) => {
                let mut set = ty.free_type_vars();
                set.extend(p.free_type_vars());
                set.extend(eq.free_type_vars());
                set.extend(proof.free_type_vars());
                set
            }

            // Phase 2A terms
            Term::StringLit(_) => HashSet::new(),
            Term::StrConcat(t1, t2) | Term::StrEq(t1, t2) => {
                let mut set = t1.free_type_vars();
                set.extend(t2.free_type_vars());
                set
            }
            Term::StrLen(t) => t.free_type_vars(),
            Term::Fix(_, ty, body) => {
                let mut set = ty.free_type_vars();
                set.extend(body.free_type_vars());
                set
            }
            Term::Fold(ty, t) | Term::Unfold(ty, t) => {
                let mut set = ty.free_type_vars();
                set.extend(t.free_type_vars());
                set
            }

            // Phase 3C: Arithmetic
            Term::NatAdd(t1, t2)
            | Term::NatSub(t1, t2)
            | Term::NatMul(t1, t2)
            | Term::NatDiv(t1, t2)
            | Term::NatMod(t1, t2)
            | Term::NatEq(t1, t2)
            | Term::BoolAnd(t1, t2)
            | Term::BoolOr(t1, t2) => {
                let mut set = t1.free_type_vars();
                set.extend(t2.free_type_vars());
                set
            }
            Term::BoolNot(t) => t.free_type_vars(),

            // Phase 3-Prep terms
            Term::NatLt(t1, t2)
            | Term::NatLe(t1, t2)
            | Term::NatGt(t1, t2)
            | Term::NatGe(t1, t2) => {
                let mut set = t1.free_type_vars();
                set.extend(t2.free_type_vars());
                set
            }
            Term::StrCharAt(s, idx) => {
                let mut set = s.free_type_vars();
                set.extend(idx.free_type_vars());
                set
            }
            Term::StrSubstring(s, start, len) => {
                let mut set = s.free_type_vars();
                set.extend(start.free_type_vars());
                set.extend(len.free_type_vars());
                set
            }
            Term::ExternCall(_, args) => {
                let mut set = HashSet::new();
                for arg in args {
                    set.extend(arg.free_type_vars());
                }
                set
            }
            Term::RefNew(t) => t.free_type_vars(),
            Term::RefGet(t) => t.free_type_vars(),
            Term::RefSet(r, v) => {
                let mut set = r.free_type_vars();
                set.extend(v.free_type_vars());
                set
            }

            // Phase 2B: Flat ADT
            Term::AdtConstruct(adt_ty, _, payload) => {
                let mut set = adt_ty.free_type_vars();
                set.extend(payload.free_type_vars());
                set
            }
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
            // Phase 3-Prep: RefNew creates a new ref cell (value once allocated)
            // ExternCall results are values once returned
            _ => false,
        }
    }
}
