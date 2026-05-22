//! Generic term traversal: visit all immediate sub-terms of a `Term` node.
//!
//! Provides `Term::for_each_subterm` which calls a visitor closure on every
//! direct child `Term`. This eliminates the need for each analysis pass to
//! duplicate the structural match over all `Term` variants.

use super::Term;

impl Term {
    /// Call `f` on every immediate sub-term of this node.
    ///
    /// Does NOT recurse — the caller is responsible for driving recursion
    /// by calling `for_each_subterm` inside their visitor if needed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn walk(term: &Term) {
    ///     // process this node...
    ///     term.for_each_subterm(|child| walk(child));
    /// }
    /// ```
    pub fn for_each_subterm(&self, mut f: impl FnMut(&Term)) {
        self.for_each_subterm_ref(&mut f);
    }

    /// Internal helper taking `&mut` visitor to avoid closure-size bloat
    /// from recursive monomorphization.
    fn for_each_subterm_ref(&self, f: &mut impl FnMut(&Term)) {
        match self {
            // Leaves — no sub-terms
            Term::Var(_)
            | Term::Global(_)
            | Term::Zero
            | Term::NatLit(_)
            | Term::True
            | Term::False
            | Term::Unit
            | Term::StringLit(_)
            | Term::Sorry => {}

            // Unary — one sub-term
            Term::Succ(t)
            | Term::Fst(t)
            | Term::Snd(t)
            | Term::StrLen(t)
            | Term::BoolNot(t)
            | Term::RefNew(t)
            | Term::RefGet(t)
            | Term::Return(t)
            | Term::Fold(_, t)
            | Term::Unfold(_, t)
            | Term::Inl(_, t)
            | Term::Inr(_, t)
            | Term::Absurd(_, t)
            | Term::Refl(_, t)
            | Term::Annot(t, _)
            | Term::TyApp(t, _)
            | Term::TyAbs(_, t)
            | Term::Spanned(t, _)
            | Term::AdtConstruct(_, _, t) => f(t),

            // Unary binding — one sub-term under a binder
            Term::Lambda(_, _, body) | Term::Fix(_, _, body) => f(body),

            // Binary — two sub-terms
            Term::App(a, b)
            | Term::Pair(a, b)
            | Term::StrConcat(a, b)
            | Term::StrEq(a, b)
            | Term::StrCharAt(a, b)
            | Term::NatAdd(a, b)
            | Term::NatSub(a, b)
            | Term::NatMul(a, b)
            | Term::NatDiv(a, b)
            | Term::NatMod(a, b)
            | Term::NatEq(a, b)
            | Term::NatLt(a, b)
            | Term::NatLe(a, b)
            | Term::NatGt(a, b)
            | Term::NatGe(a, b)
            | Term::BoolAnd(a, b)
            | Term::BoolOr(a, b)
            | Term::RefSet(a, b)
            | Term::Subst(_, _, a, b) => {
                f(a);
                f(b);
            }

            // Binary binding — value + body
            Term::Let(_, _, val, body) => {
                f(val);
                f(body);
            }

            // Ternary
            Term::If(a, b, c)
            | Term::StrSubstring(a, b, c)
            | Term::NatRec(_, a, b, c)
            | Term::NatInd(_, a, b, c) => {
                f(a);
                f(b);
                f(c);
            }

            // Case — scrutinee + two branches
            Term::Case(scrut, _, left, _, right) => {
                f(scrut);
                f(left);
                f(right);
            }

            // ADT match — scrutinee + arm bodies
            Term::AdtMatch(scrut, arms) => {
                f(scrut);
                for (_, _, body) in arms {
                    f(body);
                }
            }

            // Variadic
            Term::ExternCall(_, args) => {
                for arg in args {
                    f(arg);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    #[test]
    fn test_leaf_has_no_subterms() {
        let mut count = 0;
        Term::NatLit(42).for_each_subterm(|_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_unary_has_one_subterm() {
        let term = Term::succ(Term::Zero);
        let mut count = 0;
        term.for_each_subterm(|_| count += 1);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_binary_has_two_subterms() {
        let term = Term::app(Term::Zero, Term::NatLit(1));
        let mut count = 0;
        term.for_each_subterm(|_| count += 1);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_let_visits_both_value_and_body() {
        let term = Term::let_in("x", Type::Nat, Term::Zero, Term::NatLit(1));
        let mut count = 0;
        term.for_each_subterm(|_| count += 1);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_adt_match_visits_scrutinee_and_arms() {
        let term = Term::adt_match(
            Term::Zero,
            vec![
                (0, "x".to_string(), Box::new(Term::NatLit(1))),
                (1, "y".to_string(), Box::new(Term::NatLit(2))),
            ],
        );
        let mut count = 0;
        term.for_each_subterm(|_| count += 1);
        // 1 scrutinee + 2 arm bodies = 3
        assert_eq!(count, 3);
    }

    #[test]
    fn test_recursive_walk_counts_all_nodes() {
        // Build: App(Succ(Zero), NatLit(1))
        let term = Term::app(Term::succ(Term::Zero), Term::NatLit(1));
        let mut count = 0;
        fn walk(t: &Term, count: &mut usize) {
            *count += 1;
            t.for_each_subterm(|child| walk(child, count));
        }
        walk(&term, &mut count);
        // App(Succ(Zero), NatLit(1)) = 4 nodes
        assert_eq!(count, 4);
    }
}
