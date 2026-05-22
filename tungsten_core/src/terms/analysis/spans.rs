//! Span stripping and sorry detection.
//!
//! Structural recursion over term trees for span removal and axiom-hole checking.

use crate::terms::Term;

impl Term {
    /// Recursively strip all `Spanned` wrappers from a term tree.
    ///
    /// The evaluator is a reduction engine that doesn't need source spans.
    /// Stripping spans before evaluation prevents `Spanned` wrappers from
    /// blocking pattern matches in step handlers (e.g., `Fst(Spanned(Pair(...)))`)
    /// where `is_value()` sees through `Spanned` but destructuring does not.
    #[must_use]
    pub fn strip_spans(&self) -> Term {
        match self {
            Term::Spanned(inner, _) => inner.strip_spans(),

            // Leaf terms — no subterms
            Term::Var(_)
            | Term::Global(_)
            | Term::Unit
            | Term::True
            | Term::False
            | Term::Zero
            | Term::NatLit(_)
            | Term::StringLit(_)
            | Term::Sorry => self.clone(),

            // Unary — one subterm
            Term::Succ(t) => Term::succ(t.strip_spans()),
            Term::Fst(t) => Term::fst(t.strip_spans()),
            Term::Snd(t) => Term::snd(t.strip_spans()),
            Term::Inl(ty, t) => Term::inl(ty.clone(), t.strip_spans()),
            Term::Inr(ty, t) => Term::inr(ty.clone(), t.strip_spans()),
            Term::Refl(ty, t) => Term::refl(ty.clone(), t.strip_spans()),
            Term::Absurd(ty, t) => Term::absurd(ty.clone(), t.strip_spans()),
            Term::Annot(t, ty) => Term::annot(t.strip_spans(), ty.clone()),
            Term::TyApp(t, ty) => Term::ty_app(t.strip_spans(), ty.clone()),
            Term::StrLen(t) => Term::str_len(t.strip_spans()),
            Term::BoolNot(t) => Term::bool_not(t.strip_spans()),
            Term::Fold(ty, t) => Term::fold(ty.clone(), t.strip_spans()),
            Term::Unfold(ty, t) => Term::unfold(ty.clone(), t.strip_spans()),
            Term::RefNew(t) => Term::ref_new(t.strip_spans()),
            Term::RefGet(t) => Term::ref_get(t.strip_spans()),
            Term::Return(t) => Term::early_return(t.strip_spans()),
            Term::AdtConstruct(ty, tag, payload) => {
                Term::adt_construct(ty.clone(), *tag, payload.strip_spans())
            }

            // Unary binding
            Term::Lambda(x, ty, body) => Term::lambda(x, ty.clone(), body.strip_spans()),
            Term::TyAbs(x, body) => Term::ty_abs(x, body.strip_spans()),
            Term::Fix(f, ty, body) => Term::fix(f.clone(), ty.clone(), body.strip_spans()),

            // Binary
            Term::App(a, b) => Term::app(a.strip_spans(), b.strip_spans()),
            Term::Pair(a, b) => Term::pair(a.strip_spans(), b.strip_spans()),
            Term::StrConcat(a, b) => Term::str_concat(a.strip_spans(), b.strip_spans()),
            Term::StrEq(a, b) => Term::str_eq(a.strip_spans(), b.strip_spans()),
            Term::StrCharAt(a, b) => Term::str_char_at(a.strip_spans(), b.strip_spans()),
            Term::NatAdd(a, b) => Term::nat_add(a.strip_spans(), b.strip_spans()),
            Term::NatSub(a, b) => Term::nat_sub(a.strip_spans(), b.strip_spans()),
            Term::NatMul(a, b) => Term::nat_mul(a.strip_spans(), b.strip_spans()),
            Term::NatDiv(a, b) => Term::nat_div(a.strip_spans(), b.strip_spans()),
            Term::NatMod(a, b) => Term::nat_mod(a.strip_spans(), b.strip_spans()),
            Term::NatEq(a, b) => Term::nat_eq(a.strip_spans(), b.strip_spans()),
            Term::NatLt(a, b) => Term::nat_lt(a.strip_spans(), b.strip_spans()),
            Term::NatLe(a, b) => Term::nat_le(a.strip_spans(), b.strip_spans()),
            Term::NatGt(a, b) => Term::nat_gt(a.strip_spans(), b.strip_spans()),
            Term::NatGe(a, b) => Term::nat_ge(a.strip_spans(), b.strip_spans()),
            Term::BoolAnd(a, b) => Term::bool_and(a.strip_spans(), b.strip_spans()),
            Term::BoolOr(a, b) => Term::bool_or(a.strip_spans(), b.strip_spans()),
            Term::RefSet(a, b) => Term::ref_set(a.strip_spans(), b.strip_spans()),
            Term::Subst(x, ty, a, b) => {
                Term::subst(x.clone(), ty.clone(), a.strip_spans(), b.strip_spans())
            }

            // Binary binding
            Term::Let(x, ty, v, b) => Term::let_in(x, ty.clone(), v.strip_spans(), b.strip_spans()),

            // Ternary
            Term::If(c, t, e) => {
                Term::if_then_else(c.strip_spans(), t.strip_spans(), e.strip_spans())
            }
            Term::StrSubstring(s, start, len) => {
                Term::str_substring(s.strip_spans(), start.strip_spans(), len.strip_spans())
            }

            // Case — 5 sub-terms
            Term::Case(scrut, x, left, y, right) => Term::case(
                scrut.strip_spans(),
                x.clone(),
                left.strip_spans(),
                y.clone(),
                right.strip_spans(),
            ),

            // NatRec/NatInd — 4 sub-terms
            Term::NatRec(ty, z, s, n) => Term::natrec(
                ty.clone(),
                z.strip_spans(),
                s.strip_spans(),
                n.strip_spans(),
            ),
            Term::NatInd(m, z, s, n) => {
                Term::natind(m.clone(), z.strip_spans(), s.strip_spans(), n.strip_spans())
            }

            // ADT match
            Term::AdtMatch(scrut, arms) => Term::adt_match(
                scrut.strip_spans(),
                arms.iter()
                    .map(|(name, var, body)| (*name, var.clone(), Box::new(body.strip_spans())))
                    .collect(),
            ),

            // ExternCall — strip spans on arguments
            Term::ExternCall(name, args) => {
                Term::extern_call(name.clone(), args.iter().map(Term::strip_spans).collect())
            }
        }
    }

    /// Check if term contains `sorry` (an axiom-like hole).
    #[must_use]
    pub fn contains_sorry(&self) -> bool {
        match self {
            Term::Sorry => true,

            // Leaf terms — no subterms
            Term::Var(_)
            | Term::Global(_)
            | Term::Unit
            | Term::True
            | Term::False
            | Term::Zero
            | Term::NatLit(_)
            | Term::StringLit(_) => false,

            // Unary — one subterm
            Term::Succ(t)
            | Term::TyApp(t, _)
            | Term::Fst(t)
            | Term::Snd(t)
            | Term::Inl(_, t)
            | Term::Inr(_, t)
            | Term::Refl(_, t)
            | Term::Absurd(_, t)
            | Term::Annot(t, _)
            | Term::StrLen(t)
            | Term::BoolNot(t)
            | Term::Fold(_, t)
            | Term::Unfold(_, t)
            | Term::RefNew(t)
            | Term::RefGet(t)
            | Term::Return(t)
            | Term::AdtConstruct(_, _, t)
            | Term::Spanned(t, _) => t.contains_sorry(),

            // Unary binding — one subterm under a binder
            Term::Lambda(_, _, body) | Term::TyAbs(_, body) | Term::Fix(_, _, body) => {
                body.contains_sorry()
            }

            // Binary — two subterms
            Term::App(a, b)
            | Term::Pair(a, b)
            | Term::StrConcat(a, b)
            | Term::StrEq(a, b)
            | Term::StrCharAt(a, b)
            | Term::NatLt(a, b)
            | Term::NatLe(a, b)
            | Term::NatGt(a, b)
            | Term::NatGe(a, b)
            | Term::NatAdd(a, b)
            | Term::NatSub(a, b)
            | Term::NatMul(a, b)
            | Term::NatDiv(a, b)
            | Term::NatMod(a, b)
            | Term::NatEq(a, b)
            | Term::BoolAnd(a, b)
            | Term::BoolOr(a, b)
            | Term::RefSet(a, b)
            | Term::Subst(_, _, a, b) => a.contains_sorry() || b.contains_sorry(),

            // Binary binding — value + body
            Term::Let(_, _, v, b) => v.contains_sorry() || b.contains_sorry(),

            // Ternary
            Term::If(a, b, c) | Term::StrSubstring(a, b, c) | Term::Case(a, _, b, _, c) => {
                a.contains_sorry() || b.contains_sorry() || c.contains_sorry()
            }
            Term::NatRec(_, z, s, n) | Term::NatInd(_, z, s, n) => {
                z.contains_sorry() || s.contains_sorry() || n.contains_sorry()
            }

            // Variadic
            Term::ExternCall(_, args) => args.iter().any(Term::contains_sorry),
            Term::AdtMatch(scrutinee, arms) => {
                scrutinee.contains_sorry() || arms.iter().any(|(_, _, body)| body.contains_sorry())
            }
        }
    }
}
