//! Term substitution
//!
//! Implements capture-avoiding substitution for term and type variables.
//!
//! Core type-theory substitution is here; extension term forms
//! (strings, arithmetic, booleans, refs, ADTs) are in [`extensions`].

mod extensions;

use crate::types::Type;

use super::Term;

impl Term {
    /// Substitute a term variable: t[x := s]
    #[must_use]
    pub fn substitute(&self, var: &str, replacement: &Term) -> Term {
        match self {
            Term::Var(v) if v == var => replacement.clone(),

            // Leaf terms: no sub-terms to substitute into
            Term::Var(_)
            | Term::Global(_)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::Zero
            | Term::NatLit(_)
            | Term::Sorry => self.clone(),

            // Binding forms with single binder + type + body
            Term::Lambda(x, ty, body) | Term::Fix(x, ty, body) => {
                let new_body = Box::new(sub_if_unshadowed(body, x, var, replacement));
                if matches!(self, Term::Lambda(..)) {
                    Term::Lambda(x.clone(), ty.clone(), new_body)
                } else {
                    Term::Fix(x.clone(), ty.clone(), new_body)
                }
            }

            Term::Let(x, ty, def, body) => {
                let new_def = def.substitute(var, replacement);
                Term::Let(
                    x.clone(),
                    ty.clone(),
                    Box::new(new_def),
                    Box::new(sub_if_unshadowed(body, x, var, replacement)),
                )
            }

            Term::If(c, t, e) => Term::if_then_else(
                c.substitute(var, replacement),
                t.substitute(var, replacement),
                e.substitute(var, replacement),
            ),

            Term::Case(scrut, x, t1, y, t2) => Term::case(
                scrut.substitute(var, replacement),
                x.clone(),
                sub_if_unshadowed(t1, x, var, replacement),
                y.clone(),
                sub_if_unshadowed(t2, y, var, replacement),
            ),

            Term::TyAbs(alpha, body) => {
                Term::ty_abs(alpha.clone(), body.substitute(var, replacement))
            }
            Term::TyApp(t, ty) => Term::ty_app(t.substitute(var, replacement), ty.clone()),

            Term::Subst(ty, p, eq, proof) => Term::subst(
                ty.clone(),
                p.clone(),
                eq.substitute(var, replacement),
                proof.substitute(var, replacement),
            ),

            Term::Annot(t, ty) => Term::annot(t.substitute(var, replacement), ty.clone()),

            // Span wrapper: preserve span through substitution
            Term::Spanned(inner, span) => {
                Term::Spanned(Box::new(inner.substitute(var, replacement)), *span)
            }

            // Structural groups: dispatch via lookup helpers
            _ => substitute_structural(self, var, replacement),
        }
    }

    /// Substitute a type variable in all type annotations: t[α := τ']
    #[must_use]
    pub fn substitute_type(&self, var: &str, replacement: &Type) -> Term {
        match self {
            // Leaf terms: no type annotations to substitute in
            Term::Var(_)
            | Term::Global(_)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::Zero
            | Term::NatLit(_)
            | Term::Sorry => self.clone(),

            Term::Lambda(x, ty, body) => Term::Lambda(
                x.clone(),
                ty.substitute(var, replacement),
                Box::new(body.substitute_type(var, replacement)),
            ),

            Term::Fix(x, ty, body) => Term::Fix(
                x.clone(),
                ty.substitute(var, replacement),
                Box::new(body.substitute_type(var, replacement)),
            ),

            Term::Let(x, ty, def, body) => Term::Let(
                x.clone(),
                ty.substitute(var, replacement),
                Box::new(def.substitute_type(var, replacement)),
                Box::new(body.substitute_type(var, replacement)),
            ),

            Term::If(c, t, e) => Term::if_then_else(
                c.substitute_type(var, replacement),
                t.substitute_type(var, replacement),
                e.substitute_type(var, replacement),
            ),

            Term::Case(scrut, x, t1, y, t2) => Term::case(
                scrut.substitute_type(var, replacement),
                x.clone(),
                t1.substitute_type(var, replacement),
                y.clone(),
                t2.substitute_type(var, replacement),
            ),

            Term::TyAbs(alpha, body) => {
                let new_body = if alpha == var {
                    body.as_ref().clone()
                } else {
                    body.substitute_type(var, replacement)
                };
                Term::ty_abs(alpha.clone(), new_body)
            }

            Term::TyApp(t, ty) => Term::ty_app(
                t.substitute_type(var, replacement),
                ty.substitute(var, replacement),
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

            // Span wrapper: preserve span through type substitution
            Term::Spanned(inner, span) => {
                Term::Spanned(Box::new(inner.substitute_type(var, replacement)), *span)
            }

            // Quad typed: type + three sub-terms
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

            // Structural groups: dispatch via lookup helpers
            _ => {
                if let Some((t, ctor)) = sub_unary_term(self) {
                    return ctor(t.substitute_type(var, replacement));
                }
                if let Some((ty, t, ctor)) = sub_typed_unary(self) {
                    return ctor(
                        ty.substitute(var, replacement),
                        t.substitute_type(var, replacement),
                    );
                }
                if let Some((a, b, ctor)) = sub_binary_terms(self) {
                    return ctor(
                        a.substitute_type(var, replacement),
                        b.substitute_type(var, replacement),
                    );
                }
                // Phase 2A/3C/3-Prep/2B terms delegated to extensions module
                extensions::substitute_type_ext(self, var, replacement)
            }
        }
    }

    /// Substitute multiple type variables in all type annotations at once.
    ///
    /// Applies each substitution in the map to every type annotation in the term tree.
    /// This is the bulk version of `substitute_type` — used by the post-elaboration
    /// TyVar cleanup pass (W3.2, ADR 13.4.26b).
    #[must_use]
    pub fn substitute_type_vars(&self, subst: &std::collections::HashMap<String, Type>) -> Term {
        if subst.is_empty() {
            return self.clone();
        }
        let mut result = self.clone();
        for (var, replacement) in subst {
            result = result.substitute_type(var, replacement);
        }
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Structural lookup helpers shared by substitute() and substitute_type()
// ─────────────────────────────────────────────────────────────────────────────

/// Unary sub-term (no type annotation): `Succ`, `Fst`, `Snd`.
fn sub_unary_term(term: &Term) -> Option<(&Term, fn(Term) -> Term)> {
    match term {
        Term::Succ(t) => Some((t, Term::succ)),
        Term::Fst(t) => Some((t, Term::fst)),
        Term::Snd(t) => Some((t, Term::snd)),
        Term::Return(t) => Some((t, Term::early_return)),
        _ => None,
    }
}

/// Typed-unary: type annotation + single sub-term (`Absurd`, `Inl`, `Inr`, `Refl`).
fn sub_typed_unary(term: &Term) -> Option<(&Type, &Term, fn(Type, Term) -> Term)> {
    match term {
        Term::Absurd(ty, t) => Some((ty, t, Term::absurd)),
        Term::Inl(ty, t) => Some((ty, t, Term::inl)),
        Term::Inr(ty, t) => Some((ty, t, Term::inr)),
        Term::Refl(ty, t) => Some((ty, t, Term::refl)),
        _ => None,
    }
}

/// Binary sub-terms (no type annotation): `App`, `Pair`.
fn sub_binary_terms(term: &Term) -> Option<(&Term, &Term, fn(Term, Term) -> Term)> {
    match term {
        Term::App(a, b) => Some((a, b, Term::app)),
        Term::Pair(a, b) => Some((a, b, Term::pair)),
        _ => None,
    }
}

/// Substitute into `body` unless `binder` shadows `var`.
fn sub_if_unshadowed(body: &Term, binder: &str, var: &str, replacement: &Term) -> Term {
    if binder == var {
        body.clone()
    } else {
        body.substitute(var, replacement)
    }
}

/// Handle structural substitution via lookup helpers (unary, typed-unary, binary, quad, extensions).
fn substitute_structural(term: &Term, var: &str, replacement: &Term) -> Term {
    if let Some((t, ctor)) = sub_unary_term(term) {
        return ctor(t.substitute(var, replacement));
    }
    if let Some((ty, t, ctor)) = sub_typed_unary(term) {
        return ctor(ty.clone(), t.substitute(var, replacement));
    }
    if let Some((a, b, ctor)) = sub_binary_terms(term) {
        return ctor(
            a.substitute(var, replacement),
            b.substitute(var, replacement),
        );
    }
    // Quad typed: type + three sub-terms (NatRec, NatInd)
    match term {
        Term::NatRec(ty, z, s, n) => {
            return Term::natrec(
                ty.clone(),
                z.substitute(var, replacement),
                s.substitute(var, replacement),
                n.substitute(var, replacement),
            )
        }
        Term::NatInd(p, z, s, n) => {
            return Term::natind(
                p.clone(),
                z.substitute(var, replacement),
                s.substitute(var, replacement),
                n.substitute(var, replacement),
            )
        }
        _ => {}
    }
    extensions::substitute_ext(term, var, replacement)
}
