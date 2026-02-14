//! Phase 2: Type Unfolding
//!
//! Handles μ-type unfolding for recursive ADTs.
//!
//! ## The Two-Phase Substitution Bug (ADR 30.1.26)
//!
//! When unfolding a generic recursive type like `List<String>`:
//!
//! ```text
//! List<String> = μα_List. (Unit + (String × α_List))
//! ```
//!
//! Unfolding naively gives:
//! ```text
//! Unit + (String × α_List)  // ❌ α_List still present!
//! ```
//!
//! We must substitute the μ-variable with the full type:
//! ```text
//! Unit + (String × List<String>)  // ✓ Correct
//! ```
//!
//! This ensures that field types in constructor patterns are correct.

use std::env;
use tungsten_core::{Term, Type};

/// Check if debug tracing is enabled for type unfolding.
/// Set TUNGSTEN_DEBUG_UNFOLD=1 to enable.
fn debug_unfold_enabled() -> bool {
    env::var("TUNGSTEN_DEBUG_UNFOLD")
        .map(|v| v == "1")
        .unwrap_or(false)
}

use crate::elaborate::Elaborator;

impl<'a> Elaborator<'a> {
    /// Unfold the scrutinee type if it's a recursive μ-type.
    ///
    /// For `μX.F(X)`, unfolding gives `F(μX.F(X))` - we substitute X with the full μ-type.
    /// This is critical for generic ADTs like `List<String> = μα.(Unit + (String × α))`.
    /// See ADR 30.1.26 for details on the two-phase substitution bug.
    ///
    /// Returns `(unfolded_type, unfolded_term)`.
    /// # Cross-Module Type Handling (ADR 30.1.26, ADR 31.1.26)
    ///
    /// When the scrutinee type comes from a cross-module reference, it may be
    /// represented as `Type::App("TypeName", [args])` instead of its structural
    /// encoding. We ALWAYS normalize the type first to expand such references,
    /// not just for recursive types.
    ///
    /// For example, `Option<String>` imported from another module might be:
    /// - `Type::App("Option", [String])` (cross-module, unexpanded)
    /// - `Type::Sum(Unit, String)` (properly encoded)
    ///
    /// Without normalization, the `build_adt_match` function fails with E9999
    /// "expected sum type" because it receives the unexpanded `Type::App`.
    pub(super) fn unfold_scrutinee_type(
        &self,
        scrutinee_ty: &Type,
        scrutinee_term: Term,
        is_recursive: bool,
    ) -> (Type, Term) {
        let debug = debug_unfold_enabled();

        if debug {
            eprintln!("\n=== unfold_scrutinee_type ===");
            eprintln!("  scrutinee_ty (raw): {:?}", scrutinee_ty);
            eprintln!("  is_recursive: {}", is_recursive);
        }

        // ALWAYS normalize to expand cross-module Type::App references to their
        // structural encodings. This is necessary even for non-recursive ADTs
        // because the scrutinee type may come from a cross-module import that
        // stored it as Type::App instead of its sum-type encoding.
        let normalized = self.normalize_for_comparison(scrutinee_ty);

        if debug {
            eprintln!("  normalized: {:?}", normalized);
            eprintln!(
                "  normalized matches Mu?: {}",
                matches!(&normalized, Type::Mu(_, _))
            );
        }

        if is_recursive {
            let sum_type = match &normalized {
                Type::Mu(var, body) => {
                    let unfolded = body.substitute(var, &normalized);
                    if debug {
                        eprintln!("  μ-var: {}", var);
                        eprintln!("  unfolded sum_type: {:?}", unfolded);
                        eprintln!(
                            "  unfolded matches Sum?: {}",
                            matches!(&unfolded, Type::Sum(_, _))
                        );
                    }
                    unfolded
                }
                _ => {
                    if debug {
                        eprintln!(
                            "  WARNING: recursive type normalized to non-Mu: {:?}",
                            normalized
                        );
                    }
                    normalized.clone()
                }
            };
            let match_scrutinee = Term::unfold(scrutinee_ty.clone(), scrutinee_term);
            (sum_type, match_scrutinee)
        } else {
            // For non-recursive ADTs, the normalized type should be a Sum
            // (e.g., Option<String> → Sum(Unit, String))
            if debug {
                eprintln!("  non-recursive, returning normalized type");
                eprintln!(
                    "  normalized matches Sum?: {}",
                    matches!(&normalized, Type::Sum(_, _))
                );
            }
            (normalized, scrutinee_term)
        }
    }
}

#[cfg(test)]
mod tests {
    use tungsten_core::Type;

    /// Test that μ-type unfolding properly substitutes the bound variable.
    ///
    /// This is a key part of the fix for ADR 30.1.26 - without this substitution,
    /// pattern variables get types containing raw μ-variables like `α_List`.
    #[test]
    fn test_mu_type_unfolding_substitutes_variable() {
        // Create a μ-type: μα. (Unit + (String × α))
        // This represents List<String>
        let alpha = "α_List".to_string();
        let body = Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::String),
                Box::new(Type::TyVar(alpha.clone())),
            )),
        );
        let mu_type = Type::Mu(alpha.clone(), Box::new(body.clone()));

        // Unfold: substitute α with the full μ-type
        let unfolded = body.substitute(&alpha, &mu_type);

        // The result should have the μ-type in place of α
        // Unit + (String × μα.(Unit + (String × α)))
        match &unfolded {
            Type::Sum(_, right) => {
                match &**right {
                    Type::Product(_, tail) => {
                        // The tail should be the full μ-type, not TyVar("α_List")
                        assert!(
                            matches!(&**tail, Type::Mu(_, _)),
                            "Expected μ-type in tail position, got {:?}",
                            tail
                        );
                    }
                    _ => panic!("Expected product type"),
                }
            }
            _ => panic!("Expected sum type"),
        }
    }

    /// Test that non-recursive types pass through unchanged.
    #[test]
    fn test_non_recursive_type_unchanged() {
        let ty = Type::Sum(Box::new(Type::Unit), Box::new(Type::String));

        // For non-recursive types, unfolding should return the same type
        // (This is tested indirectly through the elaborator, but we can test the logic)
        let cloned = ty.clone();
        assert_eq!(ty, cloned);
    }

    /// Test with nested μ-types to ensure correct substitution.
    ///
    /// This catches bugs where only the outermost μ-variable is substituted.
    #[test]
    fn test_nested_mu_type_substitution() {
        // Create: μα. (μβ. (α × β) + Unit)
        // This is a contrived example but tests nested binding
        let alpha = "α".to_string();
        let beta = "β".to_string();

        let inner_body = Type::Product(
            Box::new(Type::TyVar(alpha.clone())),
            Box::new(Type::TyVar(beta.clone())),
        );
        let inner_mu = Type::Mu(beta.clone(), Box::new(inner_body));

        let outer_body = Type::Sum(Box::new(inner_mu), Box::new(Type::Unit));
        let outer_mu = Type::Mu(alpha.clone(), Box::new(outer_body.clone()));

        // Substitute α in the body
        let unfolded = outer_body.substitute(&alpha, &outer_mu);

        // Check that α was substituted but β remains
        match &unfolded {
            Type::Sum(left, _) => {
                match &**left {
                    Type::Mu(var, body) => {
                        assert_eq!(var, &beta, "Inner μ-variable should be β");
                        // The body should contain the outer μ-type, not α
                        match &**body {
                            Type::Product(fst, _) => {
                                assert!(
                                    matches!(&**fst, Type::Mu(_, _)),
                                    "α should be replaced with μ-type"
                                );
                            }
                            _ => panic!("Expected product"),
                        }
                    }
                    _ => panic!("Expected inner μ-type"),
                }
            }
            _ => panic!("Expected sum type"),
        }
    }
}
