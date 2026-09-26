//! Term/type builders for the termination tests.
//!
//! The shapes here mirror what the elaborator actually emits — verified by
//! dumping Core for the equivalent `.tg` source — so a test that passes here is
//! testing the real lowering, not a convenient idealisation of it. In
//! particular a two-constructor ADT lowers to `Case(Unfold(μ, x), …)` with the
//! payload reached through `Let`/`Fst`/`Snd`, never to a bespoke match node.

use std::collections::BTreeMap;

use crate::terms::Term;
use crate::types::Type;

use crate::terms::termination::{DefRole, DefView, TerminationAnnotation};

/// `μα. Unit + (Nat × α)` — the encoding of `type Lst = Nil | Cons(Nat, Lst)`.
#[must_use]
pub fn list_type() -> Type {
    Type::Mu(
        "α_Lst".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_Lst".to_string())),
            )),
        )),
    )
}

/// `match <scrutinee> { Nil => nil_arm, Cons(_, tail) => cons_arm }`.
///
/// `tail` is bound the way the elaborator binds it: the payload is destructured
/// into a `Let`, and the recursive field is its second projection.
#[must_use]
pub fn match_list(scrutinee: Term, nil_arm: Term, tail: &str, cons_arm: Term) -> Term {
    Term::Case(
        Box::new(Term::Unfold(list_type(), Box::new(scrutinee))),
        "__ctor_Nil".to_string(),
        Box::new(nil_arm),
        "__rest0".to_string(),
        Box::new(Term::Let(
            "__ctor_Cons".to_string(),
            Type::Product(Box::new(Type::Nat), Box::new(list_type())),
            Box::new(Term::Var("__rest0".to_string())),
            Box::new(Term::Let(
                tail.to_string(),
                list_type(),
                Box::new(Term::Snd(Box::new(Term::Var("__ctor_Cons".to_string())))),
                Box::new(cons_arm),
            )),
        )),
    )
}

/// A one-parameter function over `Lst`.
#[must_use]
pub fn list_fn(param: &str, body: Term) -> Term {
    Term::Lambda(param.to_string(), list_type(), Box::new(body))
}

/// Apply `callee` to `args`, left to right.
#[must_use]
pub fn call(callee: &str, args: Vec<Term>) -> Term {
    args.into_iter()
        .fold(Term::Global(callee.to_string()), |applied, arg| {
            Term::App(Box::new(applied), Box::new(arg))
        })
}

/// A variable reference.
#[must_use]
pub fn var(name: &str) -> Term {
    Term::Var(name.to_string())
}

/// A definition ready for `analyze`, with everything but the body defaulted.
pub struct Def {
    /// The definition's name.
    pub name: String,
    /// Its type.
    pub ty: Type,
    /// Its body.
    pub term: Term,
    /// Its annotations.
    pub annotation: TerminationAnnotation,
    /// Whether it is proof-relevant.
    pub role: DefRole,
}

impl Def {
    /// An executable definition with no annotations.
    #[must_use]
    pub fn new(name: &str, ty: Type, term: Term) -> Self {
        Def {
            name: name.to_string(),
            ty,
            term,
            annotation: TerminationAnnotation::default(),
            role: DefRole::Executable,
        }
    }

    /// The same definition, marked `#[partial]`.
    #[must_use]
    pub fn partial(mut self) -> Self {
        self.annotation.partial = true;
        self
    }

    /// The same definition, with an explicit decreasing parameter.
    #[must_use]
    pub fn decreasing(mut self, parameter: &str) -> Self {
        self.annotation.decreasing = Some(parameter.to_string());
        self
    }

    /// The same definition, treated as a proof.
    #[must_use]
    pub fn proof(mut self) -> Self {
        self.role = DefRole::Proof;
        self
    }
}

/// `Lst -> Nat`, the signature most fixtures here have.
#[must_use]
pub fn list_to_nat() -> Type {
    Type::Arrow(Box::new(list_type()), Box::new(Type::Nat))
}

/// `fn len(l) = match l { Nil => 0, Cons(_, t) => len(t) }` — the canonical
/// structural recursor.
#[must_use]
pub fn structural_len() -> Def {
    Def::new(
        "len",
        list_to_nat(),
        list_fn(
            "l",
            match_list(var("l"), Term::Zero, "t", call("len", vec![var("t")])),
        ),
    )
}

/// The single failure reason recorded for `name`, or a panic naming what came
/// back instead — so a test that expects one rejection cannot pass on two.
#[must_use]
pub fn sole_failure(defs: &[Def], name: &str) -> crate::terms::termination::FailureReason {
    let report = crate::terms::termination::analyze(&views(defs));
    let matching: Vec<&crate::terms::termination::FailureReason> = report
        .failures
        .iter()
        .filter(|failure| failure.function == name)
        .map(|failure| &failure.reason)
        .collect();
    assert_eq!(matching.len(), 1, "failures: {:?}", report.failures);
    matching[0].clone()
}

/// Borrow a set of definitions as the map `analyze` takes.
#[must_use]
pub fn views(defs: &[Def]) -> BTreeMap<String, DefView<'_>> {
    defs.iter()
        .map(|def| {
            (
                def.name.clone(),
                DefView {
                    ty: &def.ty,
                    term: &def.term,
                    annotation: &def.annotation,
                    role: def.role,
                },
            )
        })
        .collect()
}
