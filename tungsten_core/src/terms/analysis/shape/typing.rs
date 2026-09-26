//! What a Core node's type is, given the types its children already reported.
//!
//! Pure, and deliberately partial: `None` means "this term records no type I
//! am willing to derive", never "this term is ill-typed". Every consumer treats
//! `None` as silence, which is what keeps [`super`]'s walk from inventing
//! findings out of the places the elaborator simply did not write a type down.
//!
//! The functions take the children's types rather than the children, so the
//! walk visits each node exactly once. Deriving a node's type by re-descending
//! into its operand — the obvious shape — is quadratic on a `match`-heavy
//! corpus, because every branch of every nested arm is re-typed once per
//! enclosing eliminator.

use crate::terms::Term;
use crate::types::{unfold_mu_type, Type};

use super::former::{former_of, Eliminator, Former};
use super::ShapeMismatch;

/// Whether an eliminator's operand has an admissible former.
///
/// `None` operand — the type was not recorded — is silence, not a finding.
#[must_use]
pub(super) fn judge(eliminator: Eliminator, operand: Option<&Type>) -> Option<ShapeMismatch> {
    let found = operand.map_or(Former::Opaque, former_of);
    if eliminator.accepts(found) {
        return None;
    }
    Some(ShapeMismatch { eliminator, found })
}

/// Judge the eliminators whose children the generic traversal visits in order.
///
/// `Case` is absent because its arms bind, so the walk handles it itself and
/// calls [`judge`] directly with the scrutinee's type.
#[must_use]
pub(super) fn eliminator_check(term: &Term, children: &[Option<Type>]) -> Option<ShapeMismatch> {
    let operand = children.first()?.as_ref();
    match term {
        Term::Fst(_) => judge(Eliminator::Fst, operand),
        Term::Snd(_) => judge(Eliminator::Snd, operand),
        Term::App(_, _) => judge(Eliminator::App, operand),
        Term::Unfold(_, _) => judge(Eliminator::Unfold, operand),
        _ => None,
    }
}

/// The recorded type of a non-binding node.
///
/// The binding forms (`Lambda`, `Fix`, `Let`, `Case`, `AdtMatch`) are handled
/// by the walk, which owns the environment; everything else is decided here
/// from annotations and children alone.
#[must_use]
pub(super) fn simple_type(term: &Term, children: &[Option<Type>]) -> Option<Type> {
    ground_type(term)
        .or_else(|| annotated_type(term))
        .or_else(|| derived_type(term, children))
}

/// The types a term form fixes outright, with nothing to inspect.
fn ground_type(term: &Term) -> Option<Type> {
    match term {
        Term::True
        | Term::False
        | Term::NatEq(_, _)
        | Term::NatLt(_, _)
        | Term::NatLe(_, _)
        | Term::NatGt(_, _)
        | Term::NatGe(_, _)
        | Term::BoolAnd(_, _)
        | Term::BoolOr(_, _)
        | Term::BoolNot(_)
        | Term::StrEq(_, _) => Some(Type::Bool),
        Term::Zero
        | Term::Succ(_)
        | Term::NatLit(_)
        | Term::NatAdd(_, _)
        | Term::NatSub(_, _)
        | Term::NatMul(_, _)
        | Term::NatDiv(_, _)
        | Term::NatMod(_, _)
        | Term::StrLen(_)
        | Term::StrCharAt(_, _)
        | Term::IntToNat(_) => Some(Type::Nat),
        Term::IntLit(_) | Term::IntNeg(_) | Term::NatToInt(_) => Some(Type::Int),
        Term::IntBin(op, _, _) => Some(if op.is_comparison() {
            Type::Bool
        } else {
            Type::Int
        }),
        Term::StringLit(_) | Term::StrConcat(_, _) | Term::StrSubstring(_, _, _) => {
            Some(Type::String)
        }
        Term::Unit | Term::RefSet(_, _) => Some(Type::Unit),
        _ => None,
    }
}

/// The forms whose own annotation IS their type.
///
/// `Unfold`'s annotation is the μ type it consumes, so the node's type is that
/// type unfolded once — the same rule the evaluator applies.
fn annotated_type(term: &Term) -> Option<Type> {
    match term {
        Term::Absurd(ty, _)
        | Term::NatRec(ty, _, _, _)
        | Term::NatInd(ty, _, _, _)
        | Term::Fold(ty, _)
        | Term::Inl(ty, _)
        | Term::Inr(ty, _)
        | Term::AdtConstruct(ty, _, _)
        | Term::Annot(_, ty) => Some(ty.clone()),
        Term::Unfold(ty, _) => Some(unfold_mu_type(ty)),
        Term::Refl(ty, witness) => Some(Type::Eq(
            Box::new(ty.clone()),
            witness.clone(),
            witness.clone(),
        )),
        _ => None,
    }
}

/// The forms whose type follows from their children's.
fn derived_type(term: &Term, children: &[Option<Type>]) -> Option<Type> {
    let first = children.first().and_then(Option::as_ref);
    match term {
        Term::Spanned(_, _) => first.cloned(),
        Term::App(_, _) => arrow_result(first),
        Term::Fst(_) => product_side(first, true),
        Term::Snd(_) => product_side(first, false),
        Term::RefGet(_) => ref_inner(first),
        Term::RefNew(_) => Some(Type::Ref(Box::new(first?.clone()))),
        Term::TyAbs(parameter, _) => {
            Some(Type::Forall(parameter.clone(), Box::new(first?.clone())))
        }
        Term::Pair(_, _) => pair_type(children),
        Term::If(_, _, _) => agreed(children.get(1..3)?),
        _ => None,
    }
}

fn pair_type(children: &[Option<Type>]) -> Option<Type> {
    let left = children.first()?.as_ref()?;
    let right = children.get(1)?.as_ref()?;
    Some(Type::Product(
        Box::new(left.clone()),
        Box::new(right.clone()),
    ))
}

/// The result half of an arrow.
#[must_use]
pub(super) fn arrow_result(ty: Option<&Type>) -> Option<Type> {
    match ty? {
        Type::Arrow(_, result) => Some((**result).clone()),
        _ => None,
    }
}

/// One side of a product; `first` selects `fst`'s side.
#[must_use]
pub(super) fn product_side(ty: Option<&Type>, first: bool) -> Option<Type> {
    let Type::Product(left, right) = ty? else {
        return None;
    };
    Some(if first {
        (**left).clone()
    } else {
        (**right).clone()
    })
}

/// The cell type of a `Ref`.
fn ref_inner(ty: Option<&Type>) -> Option<Type> {
    match ty? {
        Type::Ref(inner) => Some((**inner).clone()),
        _ => None,
    }
}

/// Both sides of a sum, or two silences.
#[must_use]
pub(super) fn sum_sides(ty: Option<&Type>) -> (Option<Type>, Option<Type>) {
    match ty {
        Some(Type::Sum(left, right)) => (Some((**left).clone()), Some((**right).clone())),
        _ => (None, None),
    }
}

/// An ADT's variant payloads, when the scrutinee's type records them.
#[must_use]
pub(super) fn adt_variants(ty: Option<&Type>) -> Option<&[(String, Type)]> {
    match ty? {
        Type::Adt(_, _, variants) => Some(variants),
        _ => None,
    }
}

/// The one type every branch that recorded a type agrees on.
///
/// Disagreement is silence rather than a pick. A `match` whose arms record
/// different types is either genuinely ill-typed — in which case the fault is
/// reported where it is *eliminated*, not invented here — or has one arm the
/// derivation simply got wrong, and propagating that wrong type is how a
/// shape check earns a reputation for noise.
#[must_use]
pub(super) fn agreed(candidates: &[Option<Type>]) -> Option<Type> {
    let mut distinct: Vec<&Type> = Vec::new();
    for candidate in candidates.iter().flatten() {
        if !distinct.contains(&candidate) {
            distinct.push(candidate);
        }
    }
    match distinct.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}
