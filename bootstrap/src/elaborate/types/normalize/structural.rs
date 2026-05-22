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
        match (a, b) {
            // Base types: must match exactly
            (Type::Nat, Type::Nat)
            | (Type::Bool, Type::Bool)
            | (Type::String, Type::String)
            | (Type::Unit, Type::Unit)
            | (Type::Void, Type::Void)
            | (Type::Prop, Type::Prop) => true,

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
            _ => false,
        }
    }
}
