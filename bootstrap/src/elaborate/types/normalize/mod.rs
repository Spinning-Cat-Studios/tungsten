//! Type Normalization and Comparison
//!
//! This module provides centralized type normalization for structural comparison.
//! All structural type comparisons should go through this module to ensure
//! consistent handling of `Type::App` expansion and other normalization.
//!
//! # Key Invariant
//!
//! All structural type comparisons MUST use `normalize_for_comparison` to ensure
//! that `Type::App` nodes are expanded to their encoded forms before comparison.
//! This prevents comparison failures when the same type is represented differently
//! (e.g., `Type::App("Point", [])` vs `Type::Product(Nat, Nat)`).
//!
//! # Cycle Detection
//!
//! Mutually recursive types (e.g., `type A = ... B ...` and `type B = ... A ...`)
//! would cause infinite recursion during normalization. We prevent this by tracking
//! types currently being expanded. If we encounter a type that's already being
//! expanded, we return it unexpanded (as `Type::App`) to break the cycle.
//!
//! # Module Organization
//!
//! - `mod.rs` — Public API and entry points
//! - `impl.rs` — Core normalization implementation
//! - `adt.rs` — ADT encoding for normalization
//! - `field.rs` — Field normalization and canonicalization
//! - `structural.rs` — Structural equality comparison
//!
//! See ADR 25.1.26.Tungsten-Type-Checker-Totality.md for background.

mod adt;
mod field;
mod r#impl;
mod structural;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::elaborate::Elaborator;
use tungsten_core::Type;

/// Context for normalizing fields within an ADT constructor.
///
/// Bundles the ADT identity, substitution map, recursion info,
/// and cycle-detection set threaded through field normalization.
pub(super) struct NormFieldCtx<'a> {
    pub adt_name: &'a str,
    pub subst: &'a HashMap<&'a str, &'a Type>,
    pub is_recursive: bool,
    pub mu_var: &'a str,
    pub in_progress: &'a mut HashSet<String>,
}

impl<'a> Elaborator<'a> {
    /// Normalize a type for structural comparison.
    ///
    /// This function expands `Type::App` to encoded forms and resolves type aliases,
    /// ensuring that structurally equivalent types can be compared correctly.
    ///
    /// # What this normalizes
    ///
    /// - `Type::App("ADTName", args)` → sum-type encoding
    /// - `Type::App("RecordName", args)` → product-type encoding  
    /// - `Type::App("AliasName", args)` → recursively normalized aliased type
    /// - Compound types (Product, Sum, Arrow, Mu, Forall) → recursively normalized
    ///
    /// # Cycle Detection
    ///
    /// This function handles mutually recursive types by tracking which types are
    /// currently being expanded. If a cycle is detected (expanding a type that's
    /// already in progress), the type is returned unexpanded to break the cycle.
    ///
    /// # Example
    ///
    /// ```text
    /// Type::App("Point", []) → Type::Product(Nat, Nat)  // if Point = { x: Nat, y: Nat }
    /// Type::App("Option", [Nat]) → Type::Sum(Unit, Nat) // if Option<T> = None | Some(T)
    /// ```
    pub fn normalize_for_comparison(&self, ty: &Type) -> Type {
        let mut in_progress = HashSet::new();
        self.normalize_for_comparison_impl(ty, &mut in_progress)
    }

    /// Check if two types are structurally equal after normalization.
    ///
    /// This is the canonical way to compare types structurally. It handles
    /// `Type::App` expansion automatically.
    ///
    /// # When to use this vs `types_equal`
    ///
    /// - Use `types_equal` (α-equivalence) for semantic type equality in type checking
    /// - Use `types_structurally_equal_normalized` for comparing type encodings/patterns
    ///
    /// # Example
    ///
    /// ```text
    /// types_structurally_equal_normalized(
    ///     Type::App("Point", []),
    ///     Type::Product(Nat, Nat)
    /// ) == true  // if Point = { x: Nat, y: Nat }
    /// ```
    pub fn types_structurally_equal_normalized(&self, a: &Type, b: &Type) -> bool {
        let a_normalized = self.normalize_for_comparison(a);
        let b_normalized = self.normalize_for_comparison(b);

        self.types_structurally_equal_impl(&a_normalized, &b_normalized)
    }
}
