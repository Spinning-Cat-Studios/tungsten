//! Term substitution
//!
//! Implements capture-avoiding substitution for term and type variables.

use crate::types::Type;

use super::Term;

impl Term {
    /// Substitute a term variable: t[x := s]
    #[must_use]
    pub fn substitute(&self, var: &str, replacement: &Term) -> Term {
        match self {
            Term::Var(v) if v == var => replacement.clone(),
            Term::Var(v) => Term::Var(v.clone()),

            // Global references are not affected by local variable substitution
            Term::Global(name) => Term::Global(name.clone()),

            Term::Lambda(x, ty, body) if x == var => {
                // Variable is shadowed
                Term::Lambda(x.clone(), ty.clone(), body.clone())
            }
            Term::Lambda(x, ty, body) => {
                // TODO: Proper capture-avoiding substitution
                Term::Lambda(
                    x.clone(),
                    ty.clone(),
                    Box::new(body.substitute(var, replacement)),
                )
            }

            Term::App(t1, t2) => Term::app(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),

            Term::Let(x, ty, def, body) => {
                let new_def = def.substitute(var, replacement);
                if x == var {
                    // Variable is shadowed in body
                    Term::Let(x.clone(), ty.clone(), Box::new(new_def), body.clone())
                } else {
                    Term::Let(
                        x.clone(),
                        ty.clone(),
                        Box::new(new_def),
                        Box::new(body.substitute(var, replacement)),
                    )
                }
            }

            Term::True => Term::True,
            Term::False => Term::False,
            Term::If(c, t, e) => Term::if_then_else(
                c.substitute(var, replacement),
                t.substitute(var, replacement),
                e.substitute(var, replacement),
            ),

            Term::Unit => Term::Unit,
            Term::Absurd(ty, t) => Term::absurd(ty.clone(), t.substitute(var, replacement)),

            Term::Zero => Term::Zero,
            Term::Succ(t) => Term::succ(t.substitute(var, replacement)),
            Term::NatLit(n) => Term::NatLit(*n),
            Term::NatRec(ty, z, s, n) => Term::natrec(
                ty.clone(),
                z.substitute(var, replacement),
                s.substitute(var, replacement),
                n.substitute(var, replacement),
            ),
            Term::NatInd(p, z, s, n) => Term::natind(
                p.clone(),
                z.substitute(var, replacement),
                s.substitute(var, replacement),
                n.substitute(var, replacement),
            ),

            Term::Pair(t1, t2) => Term::pair(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),
            Term::Fst(t) => Term::fst(t.substitute(var, replacement)),
            Term::Snd(t) => Term::snd(t.substitute(var, replacement)),

            Term::Inl(ty, t) => Term::inl(ty.clone(), t.substitute(var, replacement)),
            Term::Inr(ty, t) => Term::inr(ty.clone(), t.substitute(var, replacement)),
            Term::Case(scrut, x, t1, y, t2) => {
                let new_scrut = scrut.substitute(var, replacement);
                let new_t1 = if x == var {
                    t1.as_ref().clone()
                } else {
                    t1.substitute(var, replacement)
                };
                let new_t2 = if y == var {
                    t2.as_ref().clone()
                } else {
                    t2.substitute(var, replacement)
                };
                Term::case(new_scrut, x.clone(), new_t1, y.clone(), new_t2)
            }

            Term::TyAbs(alpha, body) => {
                Term::ty_abs(alpha.clone(), body.substitute(var, replacement))
            }
            Term::TyApp(t, ty) => Term::ty_app(t.substitute(var, replacement), ty.clone()),

            Term::Refl(ty, t) => Term::refl(ty.clone(), t.substitute(var, replacement)),
            Term::Subst(ty, p, eq, proof) => Term::subst(
                ty.clone(),
                p.clone(),
                eq.substitute(var, replacement),
                proof.substitute(var, replacement),
            ),

            Term::Annot(t, ty) => Term::annot(t.substitute(var, replacement), ty.clone()),
            Term::Sorry => Term::Sorry,

            // Phase 2A terms
            Term::StringLit(s) => Term::StringLit(s.clone()),
            Term::StrConcat(t1, t2) => Term::str_concat(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),
            Term::StrLen(t) => Term::str_len(t.substitute(var, replacement)),
            Term::StrEq(t1, t2) => Term::str_eq(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),

            Term::Fix(f, ty, body) if f == var => {
                // Variable is shadowed
                Term::Fix(f.clone(), ty.clone(), body.clone())
            }
            Term::Fix(f, ty, body) => Term::Fix(
                f.clone(),
                ty.clone(),
                Box::new(body.substitute(var, replacement)),
            ),

            Term::Fold(ty, t) => Term::fold(ty.clone(), t.substitute(var, replacement)),
            Term::Unfold(ty, t) => Term::unfold(ty.clone(), t.substitute(var, replacement)),

            // Phase 3C: Arithmetic
            Term::NatAdd(t1, t2) => Term::NatAdd(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::NatSub(t1, t2) => Term::NatSub(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::NatMul(t1, t2) => Term::NatMul(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::NatDiv(t1, t2) => Term::NatDiv(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::NatMod(t1, t2) => Term::NatMod(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::NatEq(t1, t2) => Term::NatEq(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::BoolAnd(t1, t2) => Term::BoolAnd(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::BoolOr(t1, t2) => Term::BoolOr(
                Box::new(t1.substitute(var, replacement)),
                Box::new(t2.substitute(var, replacement)),
            ),
            Term::BoolNot(t) => Term::BoolNot(Box::new(t.substitute(var, replacement))),

            // Phase 3-Prep terms
            Term::NatLt(t1, t2) => Term::nat_lt(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),
            Term::NatLe(t1, t2) => Term::nat_le(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),
            Term::NatGt(t1, t2) => Term::nat_gt(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),
            Term::NatGe(t1, t2) => Term::nat_ge(
                t1.substitute(var, replacement),
                t2.substitute(var, replacement),
            ),
            Term::StrCharAt(s, idx) => Term::str_char_at(
                s.substitute(var, replacement),
                idx.substitute(var, replacement),
            ),
            Term::StrSubstring(s, start, len) => Term::str_substring(
                s.substitute(var, replacement),
                start.substitute(var, replacement),
                len.substitute(var, replacement),
            ),
            Term::ExternCall(name, args) => {
                let new_args = args
                    .iter()
                    .map(|a| a.substitute(var, replacement))
                    .collect();
                Term::ExternCall(name.clone(), new_args)
            }
            Term::RefNew(t) => Term::ref_new(t.substitute(var, replacement)),
            Term::RefGet(t) => Term::ref_get(t.substitute(var, replacement)),
            Term::RefSet(r, v) => Term::ref_set(
                r.substitute(var, replacement),
                v.substitute(var, replacement),
            ),

            // Phase 2B: Flat ADT
            Term::AdtConstruct(adt_ty, idx, payload) => {
                Term::adt_construct(adt_ty.clone(), *idx, payload.substitute(var, replacement))
            }
            Term::AdtMatch(scrut, arms) => {
                let new_scrut = scrut.substitute(var, replacement);
                let new_arms: Vec<_> = arms
                    .iter()
                    .map(|(idx, bound_var, body)| {
                        let new_body = if bound_var == var {
                            // Variable is shadowed in this arm
                            body.as_ref().clone()
                        } else {
                            body.substitute(var, replacement)
                        };
                        (*idx, bound_var.clone(), Box::new(new_body))
                    })
                    .collect();
                Term::adt_match(new_scrut, new_arms)
            }
        }
    }

    /// Substitute a type variable in all type annotations: t[α := τ']
    #[must_use]
    pub fn substitute_type(&self, var: &str, replacement: &Type) -> Term {
        match self {
            Term::Var(v) => Term::Var(v.clone()),

            // Global references don't carry type annotations that need substitution
            Term::Global(name) => Term::Global(name.clone()),

            Term::Lambda(x, ty, body) => Term::Lambda(
                x.clone(),
                ty.substitute(var, replacement),
                Box::new(body.substitute_type(var, replacement)),
            ),

            Term::App(t1, t2) => Term::app(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),

            Term::Let(x, ty, def, body) => Term::Let(
                x.clone(),
                ty.substitute(var, replacement),
                Box::new(def.substitute_type(var, replacement)),
                Box::new(body.substitute_type(var, replacement)),
            ),

            Term::True => Term::True,
            Term::False => Term::False,
            Term::If(c, t, e) => Term::if_then_else(
                c.substitute_type(var, replacement),
                t.substitute_type(var, replacement),
                e.substitute_type(var, replacement),
            ),

            Term::Unit => Term::Unit,
            Term::Absurd(ty, t) => Term::absurd(
                ty.substitute(var, replacement),
                t.substitute_type(var, replacement),
            ),

            Term::Zero => Term::Zero,
            Term::Succ(t) => Term::succ(t.substitute_type(var, replacement)),
            Term::NatLit(n) => Term::NatLit(*n),
            Term::NatRec(ty, z, s, n) => Term::natrec(
                ty.substitute(var, replacement),
                z.substitute_type(var, replacement),
                s.substitute_type(var, replacement),
                n.substitute_type(var, replacement),
            ),
            Term::NatInd(p, z, s, n) => Term::natind(
                p.substitute(var, replacement),
                z.substitute_type(var, replacement),
                s.substitute_type(var, replacement),
                n.substitute_type(var, replacement),
            ),

            Term::Pair(t1, t2) => Term::pair(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),
            Term::Fst(t) => Term::fst(t.substitute_type(var, replacement)),
            Term::Snd(t) => Term::snd(t.substitute_type(var, replacement)),

            Term::Inl(ty, t) => Term::inl(
                ty.substitute(var, replacement),
                t.substitute_type(var, replacement),
            ),
            Term::Inr(ty, t) => Term::inr(
                ty.substitute(var, replacement),
                t.substitute_type(var, replacement),
            ),
            Term::Case(scrut, x, t1, y, t2) => Term::case(
                scrut.substitute_type(var, replacement),
                x.clone(),
                t1.substitute_type(var, replacement),
                y.clone(),
                t2.substitute_type(var, replacement),
            ),

            Term::TyAbs(alpha, body) if alpha == var => {
                // Type variable is shadowed
                Term::TyAbs(alpha.clone(), body.clone())
            }
            Term::TyAbs(alpha, body) => {
                Term::ty_abs(alpha.clone(), body.substitute_type(var, replacement))
            }

            Term::TyApp(t, ty) => Term::ty_app(
                t.substitute_type(var, replacement),
                ty.substitute(var, replacement),
            ),

            Term::Refl(ty, t) => Term::refl(
                ty.substitute(var, replacement),
                t.substitute_type(var, replacement),
            ),
            Term::Subst(ty, p, eq, proof) => Term::subst(
                ty.substitute(var, replacement),
                p.substitute(var, replacement),
                eq.substitute_type(var, replacement),
                proof.substitute_type(var, replacement),
            ),

            Term::Annot(t, ty) => Term::annot(
                t.substitute_type(var, replacement),
                ty.substitute(var, replacement),
            ),
            Term::Sorry => Term::Sorry,

            // Phase 2A terms
            Term::StringLit(s) => Term::StringLit(s.clone()),
            Term::StrConcat(t1, t2) => Term::str_concat(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),
            Term::StrLen(t) => Term::str_len(t.substitute_type(var, replacement)),
            Term::StrEq(t1, t2) => Term::str_eq(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),

            Term::Fix(f, ty, body) => Term::Fix(
                f.clone(),
                ty.substitute(var, replacement),
                Box::new(body.substitute_type(var, replacement)),
            ),

            Term::Fold(ty, t) => Term::fold(
                ty.substitute(var, replacement),
                t.substitute_type(var, replacement),
            ),
            Term::Unfold(ty, t) => Term::unfold(
                ty.substitute(var, replacement),
                t.substitute_type(var, replacement),
            ),

            // Phase 3C: Arithmetic
            Term::NatAdd(t1, t2) => Term::NatAdd(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::NatSub(t1, t2) => Term::NatSub(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::NatMul(t1, t2) => Term::NatMul(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::NatDiv(t1, t2) => Term::NatDiv(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::NatMod(t1, t2) => Term::NatMod(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::NatEq(t1, t2) => Term::NatEq(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::BoolAnd(t1, t2) => Term::BoolAnd(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::BoolOr(t1, t2) => Term::BoolOr(
                Box::new(t1.substitute_type(var, replacement)),
                Box::new(t2.substitute_type(var, replacement)),
            ),
            Term::BoolNot(t) => Term::BoolNot(Box::new(t.substitute_type(var, replacement))),

            // Phase 3-Prep terms
            Term::NatLt(t1, t2) => Term::nat_lt(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),
            Term::NatLe(t1, t2) => Term::nat_le(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),
            Term::NatGt(t1, t2) => Term::nat_gt(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),
            Term::NatGe(t1, t2) => Term::nat_ge(
                t1.substitute_type(var, replacement),
                t2.substitute_type(var, replacement),
            ),
            Term::StrCharAt(s, idx) => Term::str_char_at(
                s.substitute_type(var, replacement),
                idx.substitute_type(var, replacement),
            ),
            Term::StrSubstring(s, start, len) => Term::str_substring(
                s.substitute_type(var, replacement),
                start.substitute_type(var, replacement),
                len.substitute_type(var, replacement),
            ),
            Term::ExternCall(name, args) => {
                let new_args = args
                    .iter()
                    .map(|a| a.substitute_type(var, replacement))
                    .collect();
                Term::ExternCall(name.clone(), new_args)
            }
            Term::RefNew(t) => Term::ref_new(t.substitute_type(var, replacement)),
            Term::RefGet(t) => Term::ref_get(t.substitute_type(var, replacement)),
            Term::RefSet(r, v) => Term::ref_set(
                r.substitute_type(var, replacement),
                v.substitute_type(var, replacement),
            ),

            // Phase 2B: Flat ADT
            Term::AdtConstruct(adt_ty, idx, payload) => Term::adt_construct(
                adt_ty.substitute(var, replacement),
                *idx,
                payload.substitute_type(var, replacement),
            ),
            Term::AdtMatch(scrut, arms) => {
                let new_scrut = scrut.substitute_type(var, replacement);
                let new_arms: Vec<_> = arms
                    .iter()
                    .map(|(idx, bound_var, body)| {
                        (
                            *idx,
                            bound_var.clone(),
                            Box::new(body.substitute_type(var, replacement)),
                        )
                    })
                    .collect();
                Term::adt_match(new_scrut, new_arms)
            }
        }
    }
}
