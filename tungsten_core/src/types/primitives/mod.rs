//! The primitive type set, stated once (ADR 18.9.26f).
//!
//! Two halves, held equal by a test: `primitive_name` is the wildcard-free
//! match the compiler refuses to let a new `Type` variant skip, and
//! `PRIMITIVE_TYPES` is the name-to-`Type` direction. Every site that used to
//! enumerate the primitives by hand reads one of the two, so the next
//! primitive is one arm, one row and nothing else.

use super::Type;

/// Every primitive type, by its source-level name. Order is presentation
/// only; membership is decided by [`Type::primitive_name`].
pub const PRIMITIVE_TYPES: &[(&str, Type)] = &[
    ("Nat", Type::Nat),
    ("Int", Type::Int),
    ("Bool", Type::Bool),
    ("Unit", Type::Unit),
    ("Void", Type::Void),
    ("Prop", Type::Prop),
    ("String", Type::String),
];

impl Type {
    /// The source-level name of a primitive type, `None` for every other type.
    ///
    /// Wildcard-free on purpose: a new variant fails to compile here rather
    /// than defaulting into a `_ => false` and comparing unequal to itself
    /// (the 14.9.26c trap). `Type::Error => None` is spelled, not defaulted —
    /// a `None` is transit, not a raise.
    #[must_use]
    pub fn primitive_name(&self) -> Option<&'static str> {
        match self {
            Type::Bool => Some("Bool"),
            Type::Nat => Some("Nat"),
            Type::Int => Some("Int"),
            Type::Unit => Some("Unit"),
            Type::Void => Some("Void"),
            Type::Prop => Some("Prop"),
            Type::String => Some("String"),
            Type::Arrow(..)
            | Type::Product(..)
            | Type::Sum(..)
            | Type::TyVar(_)
            | Type::Forall(..)
            | Type::Eq(..)
            | Type::Mu(..)
            | Type::Ptr(_)
            | Type::Ref(_)
            | Type::App(..)
            | Type::Adt(..)
            | Type::Error => None,
        }
    }

    /// Whether this is one of the primitive types.
    #[must_use]
    pub fn is_primitive(&self) -> bool {
        self.primitive_name().is_some()
    }

    /// The primitive type a source-level name denotes, if any.
    #[must_use]
    pub fn primitive_by_name(name: &str) -> Option<Type> {
        PRIMITIVE_TYPES
            .iter()
            .find(|(row_name, _)| *row_name == name)
            .map(|(_, ty)| ty.clone())
    }
}

#[cfg(test)]
mod tests;
