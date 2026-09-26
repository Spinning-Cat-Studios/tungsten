//! Structural equality comparison for types.
//!
//! This module provides structural equality checking after normalization.

use crate::elaborate::Elaborator;
use tungsten_core::Type;

impl<'a> Elaborator<'a> {
    /// Implementation of structural equality (after normalization).
    ///
    /// This is an internal helper - external code should use
    /// `types_structurally_equal_normalized` which handles normalization.
    pub(crate) fn types_structurally_equal_impl(&self, a: &Type, b: &Type) -> bool {
        // Primitive types: must match exactly (ADR 18.9.26f). A pattern over
        // both names rather than a match guard: a mixed pair falls through to
        // the arms below exactly as it did before.
        if let (Some(a_name), Some(b_name)) = (a.primitive_name(), b.primitive_name()) {
            return a_name == b_name;
        }
        match (a, b) {
            (Type::TyVar(n1), Type::TyVar(n2)) => n1 == n2,

            // Binary recursive types
            (Type::Product(a1, a2), Type::Product(b1, b2))
            | (Type::Sum(a1, a2), Type::Sum(b1, b2))
            | (Type::Arrow(a1, a2), Type::Arrow(b1, b2)) => {
                self.types_structurally_equal_impl(a1, b1)
                    && self.types_structurally_equal_impl(a2, b2)
            }

            // Binding types: check variable name + structural body equality
            (Type::Mu(v1, b1), Type::Mu(v2, b2)) | (Type::Forall(v1, b1), Type::Forall(v2, b2)) => {
                v1 == v2 && self.types_structurally_equal_impl(b1, b2)
            }

            (Type::App(n1, a1), Type::App(n2, a2)) => {
                // If we get here, both are unexpanded Apps (e.g., stubs)
                n1 == n2
                    && a1.len() == a2.len()
                    && a1
                        .iter()
                        .zip(a2.iter())
                        .all(|(x, y)| self.types_structurally_equal_impl(x, y))
            }

            // ADT nodes: same name, structurally-equal type args, and
            // variant-wise equal (name + field type). Before ADR 22.7.26b two
            // `Adt` values fell through to `_ => false`, so identical ADTs
            // compared unequal (21.7.26j flagged 44 healthy ADTs this way).
            (Type::Adt(n1, args1, variants1), Type::Adt(n2, args2, variants2)) => {
                n1 == n2
                    && args1.len() == args2.len()
                    && args1
                        .iter()
                        .zip(args2.iter())
                        .all(|(x, y)| self.types_structurally_equal_impl(x, y))
                    && variants1.len() == variants2.len()
                    && variants1.iter().zip(variants2.iter()).all(
                        |((variant1, field1), (variant2, field2))| {
                            variant1 == variant2
                                && self.types_structurally_equal_impl(field1, field2)
                        },
                    )
            }
            _ => false,
        }
    }
}
// Tests: structural_tests.rs
#[cfg(test)]
#[path = "structural_tests.rs"]
mod tests;
