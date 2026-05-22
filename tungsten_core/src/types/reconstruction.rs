//! Type reconstruction helpers.
//!
//! Used by type-recursive traversals to rebuild the same variant
//! from transformed children, without a match on the original.

use crate::types::Type;

impl Type {
    /// Reconstruct a binary type (Arrow, Product, or Sum) with new children.
    ///
    /// Panics if `template` is not Arrow, Product, or Sum.
    #[must_use]
    pub fn reconstruct_binary(template: &Type, a: Type, b: Type) -> Type {
        match template {
            Type::Arrow(..) => Type::arrow(a, b),
            Type::Product(..) => Type::product(a, b),
            Type::Sum(..) => Type::sum(a, b),
            _ => unreachable!("reconstruct_binary called on non-binary type"),
        }
    }

    /// Reconstruct a binding type (Forall or Mu) with a new body.
    ///
    /// Panics if `template` is not Forall or Mu.
    pub fn reconstruct_binding(template: &Type, var: impl Into<String>, body: Type) -> Type {
        match template {
            Type::Forall(..) => Type::forall(var, body),
            Type::Mu(..) => Type::mu(var, body),
            _ => unreachable!("reconstruct_binding called on non-binding type"),
        }
    }

    /// Reconstruct a wrapper type (Ptr or Ref) with a new inner type.
    ///
    /// Panics if `template` is not Ptr or Ref.
    #[must_use]
    pub fn reconstruct_wrapper(template: &Type, inner: Type) -> Type {
        match template {
            Type::Ptr(_) => Type::ptr(inner),
            Type::Ref(_) => Type::ref_ty(inner),
            _ => unreachable!("reconstruct_wrapper called on non-wrapper type"),
        }
    }
}
