//! Variable use-count analysis (ADR 19.5.26a).
//!
//! Counts occurrences of a named variable in a term tree, respecting shadowing.
//! Used by the codegen liveness gate to determine if a variable is "last-use"
//! at a particular site (count == 1 in the remaining body).

use crate::terms::Term;

impl Term {
    /// Count how many times a variable is used in a term.
    ///
    /// Respects shadowing: if a binding form (Let, Lambda, Fix, Case, AdtMatch)
    /// rebinds the same name, uses inside that scope are NOT counted.
    #[must_use]
    pub fn var_use_count(&self, name: &str) -> usize {
        match self {
            Term::Var(v) => usize::from(v == name),

            // Leaf terms
            Term::Global(_)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::Zero
            | Term::NatLit(_)
            | Term::Sorry
            | Term::StringLit(_) => 0,

            // Binding forms: stop counting if they shadow the name
            Term::Lambda(x, _, body) | Term::Fix(x, _, body) => {
                if x == name {
                    0
                } else {
                    body.var_use_count(name)
                }
            }
            Term::Let(x, _, def, body) => {
                let def_count = def.var_use_count(name);
                let body_count = if x == name {
                    0
                } else {
                    body.var_use_count(name)
                };
                def_count + body_count
            }

            // Unary
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
            | Term::Spanned(t, _)
            | Term::Return(t) => t.var_use_count(name),

            // Binary
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
            | Term::RefSet(t1, t2) => t1.var_use_count(name) + t2.var_use_count(name),

            Term::If(t1, t2, t3) | Term::StrSubstring(t1, t2, t3) => {
                t1.var_use_count(name) + t2.var_use_count(name) + t3.var_use_count(name)
            }

            Term::NatRec(_, z, s, n) | Term::NatInd(_, z, s, n) => {
                z.var_use_count(name) + s.var_use_count(name) + n.var_use_count(name)
            }
            Term::Subst(_, _, eq, proof) => eq.var_use_count(name) + proof.var_use_count(name),

            Term::Case(scrut, x, t1, y, t2) => {
                let scrut_count = scrut.var_use_count(name);
                let t1_count = if x == name { 0 } else { t1.var_use_count(name) };
                let t2_count = if y == name { 0 } else { t2.var_use_count(name) };
                scrut_count + t1_count + t2_count
            }

            Term::ExternCall(_, args) => args.iter().map(|a| a.var_use_count(name)).sum(),

            Term::AdtConstruct(_, _, payload) => payload.var_use_count(name),

            Term::AdtMatch(scrut, arms) => {
                let mut count = scrut.var_use_count(name);
                for (_, var, body) in arms {
                    if var != name {
                        count += body.var_use_count(name);
                    }
                }
                count
            }
        }
    }
}
