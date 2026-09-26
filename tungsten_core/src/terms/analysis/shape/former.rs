//! Which former a recorded type has, and which former each eliminator needs.
//!
//! Deliberately coarse (ADR 3.9.26h D2). The question is *shape agreement* —
//! does `fst` stand over something a `fst` can be taken of — not typing. A
//! second typing judgement in this repo would be a second elaborator, and both
//! defects that motivated the check are former mismatches: an `App` whose
//! operand's recorded result is a `μ`, and a `Fst` whose operand's recorded
//! type is a scalar.
//!
//! [`Former::Opaque`] is the safety valve and carries the whole false-positive
//! budget: a type variable, a deferred `App`, or the poison type says nothing
//! about the operand, so nothing is reported. A check that guessed there would
//! be noise over 2298 definitions and would be switched off within a week.

use crate::types::Type;

/// The outermost former of a type, as far as this check is willing to judge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Former {
    Arrow,
    Product,
    Sum,
    Mu,
    Forall,
    Adt,
    Ref,
    Ptr,
    Equality,
    /// A type with no structure to eliminate: `Nat`, `Bool`, `String`, …
    Ground(&'static str),
    /// Nothing this check will judge — a type variable, a deferred type
    /// application, or the poison type.
    Opaque,
}

impl Former {
    /// How a finding names this former.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Former::Arrow => "an arrow",
            Former::Product => "a product",
            Former::Sum => "a sum",
            Former::Mu => "a recursive type",
            Former::Forall => "a forall",
            Former::Adt => "an ADT",
            Former::Ref => "a ref cell",
            Former::Ptr => "a pointer",
            Former::Equality => "an equality",
            Former::Ground(name) => name,
            Former::Opaque => "an unrecorded type",
        }
    }
}

/// Classify a type by its outermost former.
///
/// `Type::App` is the elaboration-only deferred application and `Type::Error`
/// the poison; both are [`Former::Opaque`] rather than a former of their own,
/// because a definition that already failed must not also be reported here.
#[must_use]
pub fn former_of(ty: &Type) -> Former {
    match ty {
        Type::Arrow(_, _) => Former::Arrow,
        Type::Product(_, _) => Former::Product,
        Type::Sum(_, _) => Former::Sum,
        Type::Mu(_, _) => Former::Mu,
        Type::Forall(_, _) => Former::Forall,
        Type::Adt(_, _, _) => Former::Adt,
        Type::Ref(_) => Former::Ref,
        Type::Ptr(_) => Former::Ptr,
        Type::Eq(_, _, _) => Former::Equality,
        Type::TyVar(_) | Type::App(_, _) | Type::Error => Former::Opaque,
        // Every primitive is a ground former named by its source name (ADR 18.9.26f).
        Type::Bool
        | Type::Nat
        | Type::Int
        | Type::Unit
        | Type::Void
        | Type::Prop
        | Type::String => Former::Ground(ty.primitive_name().unwrap_or_default()),
    }
}

/// The five term forms whose operand this check judges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eliminator {
    Fst,
    Snd,
    App,
    Case,
    Unfold,
}

impl Eliminator {
    /// The spelling a finding uses.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Eliminator::Fst => "fst",
            Eliminator::Snd => "snd",
            Eliminator::App => "app",
            Eliminator::Case => "case",
            Eliminator::Unfold => "unfold",
        }
    }

    /// The former this eliminator's operand must have.
    #[must_use]
    pub fn required(self) -> Former {
        match self {
            Eliminator::Fst | Eliminator::Snd => Former::Product,
            Eliminator::App => Former::Arrow,
            Eliminator::Case => Former::Sum,
            Eliminator::Unfold => Former::Mu,
        }
    }

    /// Whether an operand carrying `found` is acceptable here.
    ///
    /// The two tolerated pairs are encoding choices rather than shapes:
    /// instantiation of a polymorphic operand is not always recorded as a
    /// `TyApp`, and a two-constructor ADT elaborates to a `Sum` while a wider
    /// one elaborates to `Type::Adt` — so `case` over either is normal.
    #[must_use]
    pub fn accepts(self, found: Former) -> bool {
        if found == Former::Opaque || found == self.required() {
            return true;
        }
        matches!(
            (self, found),
            (Eliminator::App, Former::Forall) | (Eliminator::Case, Former::Adt)
        )
    }
}
