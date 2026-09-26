//! Does every eliminator in a Core term stand over something it can eliminate?
//!
//! The sibling question to [`Term::free_vars`](crate::terms::Term::free_vars),
//! and the reason ADR 3.9.26h exists: `doctor check selfhost closed-terms`
//! answers "is every name bound", both defects that surfaced while closing ADR
//! 21.8.26a passed it, and both were *shapes*. A saturated constructor
//! application elaborated to `App(App(λx:(A × B). …, 9), N2)` — closed, and
//! applying a lambda's *result* to a second argument. A tuple projection
//! emitted `fst` of a scalar. Neither has a free variable; neither is visible
//! to codegen, which rebuilds CIR from the pattern and never reads the term,
//! nor to the type checker, which resolves through an environment that is in
//! scope when it runs.
//!
//! ## What it does NOT do
//!
//! It is not a second type checker (D2). It asks only whether an eliminator's
//! operand has the right **former** — `Fst`/`Snd` over a `Product`, `App` over
//! an arrow, `Case` over a `Sum`, `Unfold` over a `Mu` — using the types the
//! elaborator already wrote onto `Lambda`, `Let`, `Fold`, `Annot` and friends.
//! Where no type was recorded there is no finding: see
//! [`Former::Opaque`](former::Former::Opaque).

mod former;
mod typing;

#[cfg(test)]
mod tests;

use crate::terms::{Term, Var};
use crate::types::Type;

pub use former::{former_of, Eliminator, Former};

use typing::{adt_variants, agreed, eliminator_check, judge, simple_type, sum_sides};

/// One eliminator standing over an operand whose recorded type cannot support
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeMismatch {
    pub eliminator: Eliminator,
    pub found: Former,
}

impl ShapeMismatch {
    /// The phrase a report uses, e.g. `fst over Nat`.
    ///
    /// Deliberately the *former* and not the rendered type: a μ-encoded ADT's
    /// `Display` runs to hundreds of characters, and one census line per
    /// definition has to stay readable.
    #[must_use]
    pub fn label(&self) -> String {
        format!(
            "{} over {}",
            self.eliminator.name(),
            self.found.description()
        )
    }
}

impl Term {
    /// Every eliminator in this term whose operand's recorded type refuses it.
    ///
    /// One pass. The types flow **up** — each node reports its own type to its
    /// parent — rather than each eliminator re-deriving its operand's type by
    /// descending again, which is quadratic on the `match`-heavy bodies this
    /// check exists to walk.
    #[must_use]
    pub fn shape_mismatches(&self) -> Vec<ShapeMismatch> {
        let mut walk = Walk {
            env: Vec::new(),
            findings: Vec::new(),
        };
        walk.analyse(self);
        walk.findings
    }
}

/// The walk's state: what each variable in scope was bound at, and what has
/// been found so far.
struct Walk {
    env: Vec<(Var, Type)>,
    findings: Vec<ShapeMismatch>,
}

impl Walk {
    /// Visit `term`, reporting any mismatch under it, and return its recorded
    /// type.
    fn analyse(&mut self, term: &Term) -> Option<Type> {
        match term {
            Term::Var(name) => self.lookup(name),
            Term::Lambda(name, parameter, body) => self.lambda(name, parameter, body),
            Term::Fix(name, ty, body) => Some(self.fix(name, ty, body)),
            Term::Let(name, ty, value, body) => self.let_form(name, ty, value, body),
            Term::Case(scrutinee, left, left_body, right, right_body) => {
                self.case((scrutinee, left, left_body, right, right_body))
            }
            Term::AdtMatch(scrutinee, arms) => self.adt_match(scrutinee, arms),
            _ => self.simple(term),
        }
    }

    /// The innermost binding of `name`, if the walk recorded one.
    fn lookup(&self, name: &str) -> Option<Type> {
        self.env
            .iter()
            .rev()
            .find(|(bound, _)| bound == name)
            .map(|(_, ty)| ty.clone())
    }

    /// Everything that neither binds nor needs the environment: walk the
    /// children generically, judge, and type from what they reported.
    fn simple(&mut self, term: &Term) -> Option<Type> {
        let mut children: Vec<Option<Type>> = Vec::new();
        term.for_each_subterm(|child| {
            let child_type = self.analyse(child);
            children.push(child_type);
        });
        if let Some(mismatch) = eliminator_check(term, &children) {
            self.findings.push(mismatch);
        }
        simple_type(term, &children)
    }

    /// Walk `body` with `name` bound at `ty`, then restore the scope.
    fn scoped(&mut self, name: &Var, ty: Type, body: &Term) -> Option<Type> {
        self.env.push((name.clone(), ty));
        let body_type = self.analyse(body);
        self.env.pop();
        body_type
    }

    /// Walk `body` with `name` bound only if the binding type is known —
    /// otherwise the variable stays unrecorded, which is silence rather than a
    /// guess.
    fn branch(&mut self, name: &Var, ty: Option<Type>, body: &Term) -> Option<Type> {
        match ty {
            Some(bound) => self.scoped(name, bound, body),
            None => self.analyse(body),
        }
    }

    fn lambda(&mut self, name: &Var, parameter: &Type, body: &Term) -> Option<Type> {
        let body_type = self.scoped(name, parameter.clone(), body)?;
        Some(Type::Arrow(
            Box::new(parameter.clone()),
            Box::new(body_type),
        ))
    }

    /// `fix f:τ. t` has type τ whatever the body reports — the body is walked
    /// for its findings, and its own derived type is not the fixpoint's.
    fn fix(&mut self, name: &Var, ty: &Type, body: &Term) -> Type {
        self.scoped(name, ty.clone(), body);
        ty.clone()
    }

    fn let_form(&mut self, name: &Var, ty: &Type, bound_term: &Term, body: &Term) -> Option<Type> {
        self.analyse(bound_term);
        self.scoped(name, ty.clone(), body)
    }

    /// `case` binds one variable per side, at the scrutinee's sum components.
    ///
    /// Taken as a tuple because five positions plus `self` is one more
    /// parameter than the code-health gate allows, and destructuring here reads
    /// better than a struct nothing else constructs.
    fn case(&mut self, parts: (&Term, &Var, &Term, &Var, &Term)) -> Option<Type> {
        let (scrutinee, left, left_body, right, right_body) = parts;
        let scrutinee_type = self.analyse(scrutinee);
        if let Some(mismatch) = judge(Eliminator::Case, scrutinee_type.as_ref()) {
            self.findings.push(mismatch);
        }
        let (left_type, right_type) = sum_sides(scrutinee_type.as_ref());
        let left_result = self.branch(left, left_type, left_body);
        let right_result = self.branch(right, right_type, right_body);
        agreed(&[left_result, right_result])
    }

    /// `AdtMatch` binds each arm's payload variable at that variant's payload
    /// type. The scrutinee itself is **not** judged: a two-constructor ADT
    /// elaborates to a `Sum` and a wider one to `Type::Adt`, so "which former
    /// an ADT scrutinee carries" is an encoding question, not a shape fault.
    fn adt_match(&mut self, scrutinee: &Term, arms: &[(usize, Var, Box<Term>)]) -> Option<Type> {
        let scrutinee_type = self.analyse(scrutinee);
        let variants = adt_variants(scrutinee_type.as_ref());
        let mut results: Vec<Option<Type>> = Vec::new();
        for (index, name, body) in arms {
            let payload = variants
                .and_then(|all| all.get(*index))
                .map(|(_, ty)| ty.clone());
            let arm_type = self.branch(name, payload, body);
            results.push(arm_type);
        }
        agreed(&results)
    }
}
