//! Generic term traversal: visit all immediate sub-terms of a `Term` node.
//!
//! Provides `Term::for_each_subterm` which calls a visitor closure on every
//! direct child `Term`. This eliminates the need for each analysis pass to
//! duplicate the structural match over all `Term` variants.
//!
//! **Sibling walker:** `ffi::arena_stats::deep_term_bytes` re-enumerates the
//! same child structure (it additionally needs embedded `Type`s, string
//! capacities, and vec slabs, which this visitor does not expose). When a
//! `Term` variant is added or its children change, update BOTH matches — the
//! `walkers_agree_on_children_for_every_variant` test in `arena_stats.rs`
//! cross-checks them and fails if the size walker misses a child this
//! traversal visits.

use super::Term;

/// Visit two children in order — keeps every arm of the dispatcher one line.
fn visit_two(f: &mut impl FnMut(&Term), a: &Term, b: &Term) {
    f(a);
    f(b);
}

/// Visit three children in order.
fn visit_three(f: &mut impl FnMut(&Term), a: &Term, b: &Term, c: &Term) {
    f(a);
    f(b);
    f(c);
}

/// Visit every child of a variadic node in order.
fn visit_all<'t>(f: &mut impl FnMut(&Term), children: impl Iterator<Item = &'t Term>) {
    for child in children {
        f(child);
    }
}

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
            | Term::IntLit(_)
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
            | Term::IntNeg(t)
            | Term::NatToInt(t)
            | Term::IntToNat(t)
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
            | Term::IntBin(_, a, b)
            | Term::BoolAnd(a, b)
            | Term::BoolOr(a, b)
            | Term::RefSet(a, b)
            | Term::Subst(_, _, a, b) => visit_two(f, a, b),

            // Binary binding — value + body
            Term::Let(_, _, val, body) => visit_two(f, val, body),

            // Ternary
            Term::If(a, b, c)
            | Term::StrSubstring(a, b, c)
            | Term::NatRec(_, a, b, c)
            | Term::NatInd(_, a, b, c) => visit_three(f, a, b, c),

            // Case — scrutinee + two branches
            Term::Case(scrut, _, left, _, right) => visit_three(f, scrut, left, right),

            // ADT match — scrutinee + arm bodies
            Term::AdtMatch(scrut, arms) => {
                f(scrut);
                visit_all(f, arms.iter().map(|(_, _, body)| body.as_ref()));
            }

            // Variadic
            Term::ExternCall(_, args) => visit_all(f, args.iter()),
        }
    }

    /// Call `f` on every `Type` stored *directly* on this node.
    ///
    /// Like [`Term::for_each_subterm`] this does NOT recurse — drive recursion
    /// by calling `for_each_subterm` alongside it. Together the two visit every
    /// type embedded anywhere in a term tree, which is what lets the
    /// strict-positivity walker treat `Eq`'s witness terms as type-bearing
    /// (`types::positivity`, ADR 7.8.26e §2.1) rather than assuming terms are
    /// type-free.
    ///
    /// **Sibling walkers:** [`Term::for_each_subterm`] and
    /// `ffi::arena_stats::deep_term_bytes` enumerate the same variants; the
    /// latter also sums these embedded types. All three matches are exhaustive
    /// — when a `Term` variant gains a `Type`, add it here too.
    pub fn for_each_embedded_type(&self, mut f: impl FnMut(&crate::types::Type)) {
        match self {
            // One embedded type
            Term::Lambda(_, ty, _)
            | Term::Fix(_, ty, _)
            | Term::Let(_, ty, _, _)
            | Term::Absurd(ty, _)
            | Term::Inl(ty, _)
            | Term::Inr(ty, _)
            | Term::Refl(ty, _)
            | Term::Fold(ty, _)
            | Term::Unfold(ty, _)
            | Term::NatRec(ty, _, _, _)
            | Term::NatInd(ty, _, _, _)
            | Term::TyApp(_, ty)
            | Term::Annot(_, ty)
            | Term::AdtConstruct(ty, _, _) => f(ty),

            // Two embedded types
            Term::Subst(base, motive, _, _) => {
                f(base);
                f(motive);
            }

            // No embedded type
            Term::Var(_)
            | Term::Global(_)
            | Term::App(_, _)
            | Term::True
            | Term::False
            | Term::If(_, _, _)
            | Term::Unit
            | Term::Zero
            | Term::Succ(_)
            | Term::NatLit(_)
            | Term::NatAdd(_, _)
            | Term::NatSub(_, _)
            | Term::NatMul(_, _)
            | Term::NatDiv(_, _)
            | Term::NatMod(_, _)
            | Term::NatEq(_, _)
            | Term::NatLt(_, _)
            | Term::NatLe(_, _)
            | Term::NatGt(_, _)
            | Term::NatGe(_, _)
            | Term::IntLit(_)
            | Term::IntBin(_, _, _)
            | Term::IntNeg(_)
            | Term::NatToInt(_)
            | Term::IntToNat(_)
            | Term::BoolAnd(_, _)
            | Term::BoolOr(_, _)
            | Term::BoolNot(_)
            | Term::StringLit(_)
            | Term::StrConcat(_, _)
            | Term::StrLen(_)
            | Term::StrEq(_, _)
            | Term::StrCharAt(_, _)
            | Term::StrSubstring(_, _, _)
            | Term::Pair(_, _)
            | Term::Fst(_)
            | Term::Snd(_)
            | Term::Case(_, _, _, _, _)
            | Term::TyAbs(_, _)
            | Term::ExternCall(_, _)
            | Term::RefNew(_)
            | Term::RefGet(_)
            | Term::RefSet(_, _)
            | Term::Sorry
            | Term::AdtMatch(_, _)
            | Term::Return(_)
            | Term::Spanned(_, _) => {}
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
