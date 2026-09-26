//! The poison type's structural predicate (ADR 7.8.26d, 14.8.26g).
//!
//! `Type::Error` is the poison a failed elaboration leaves in the environment
//! so that dependents meet *something* instead of cascading.
//!
//! # Every site that meets poison is in one of three categories
//!
//! Getting this wrong is the standing failure mode, so the categories are
//! named rather than left implicit. ADR 14.8.26g shipped the producers
//! believing the compare boundary was the whole consuming half; it is not,
//! and the missing **transit** arm turned one seeded fault into 17 reported
//! errors — 15 of them `E0013` at the call sites of a poisoned callee.
//!
//! | Category | Behaviour | Sites |
//! |---|---|---|
//! | **Compare** | poison *unifies with anything* | [`crate::types::equality`] |
//! | **Transit** | poison *passes through*, result is poison, no diagnostic | `apply_args_sequentially` (application, ADR 14.8.26g); `normalize/field.rs` (field projection); the construction and destruction boundaries — record literals, constructor calls, matches, patterns, a lambda checked against poison — via `elaborate/exprs/helpers/poison.rs` (ADR 15.8.26d) |
//! | **Terminal** | poison is *refused*, loudly | `admit_core_def` (Core), `first_poisoned_export` (cache), `tungsten_codegen::types::lowering` (codegen) |
//!
//! **Transit is the category that gets forgotten**, because it is the only
//! one that is neither a comparison nor a guard: a site that destructures a
//! type to *use* it (apply it, project from it, construct with it) must
//! propagate poison silently — the fault was already reported where the
//! poison was produced, so re-diagnosing it at every use site is the cascade
//! the poison exists to prevent. Refusing there is as wrong as unifying.
//!
//! The construction boundaries were the last large gap: a record literal
//! raised `E0050` and a constructor call `E0010` at every site of a failed
//! type (V3 = 15, V5 = 24 on the seeded corpus). ADR 15.8.26d moved them
//! into Transit (V3 = 1, V5 = 1); the eighteen smaller destructures the
//! census still holds as open findings are ADR 17.9.26d's.
//!
//! **This table is now enforced, not merely documented.** `code-health`'s
//! `poison-boundaries` check (ADR 18.8.26a) censuses every `Type` /
//! `TypeDefKind` destructure with no branch for the failed case whose
//! fall-through raises, labels each against the three categories above, and
//! gates on any site nobody has reviewed. This doc stays the authority: a
//! `#[test]` in `tools/code-health` parses the rows above and fails if the
//! classifier's categories drift from them, so a fourth category cannot appear
//! in the check without appearing here. What the check CANNOT see is a boundary
//! poison never reaches — that has no arm to miss — so an empty candidate list
//! is not a claim about producer coverage.
//!
//! When adding a producer, walk all three columns before believing the
//! suppression is complete, and probe each candidate with a fixture that
//! *uses* the poisoned value with arguments — a nullary probe never enters
//! the argument loop, so it false-passes
//! (`docs/repo-memory/claude-memories/poison-consumers-need-argumentful-probes.md`).

use super::Type;

impl Type {
    /// Whether this type is, or structurally contains, the poison type.
    ///
    /// Only the `Error` leaf is non-uniform; every other variant delegates to
    /// [`Type::children`], so a future `Type` variant inherits the structural
    /// default rather than silently reading as poison-free (ADR 23.7.26b).
    ///
    /// Note the asymmetry with equality: poison *compares* equal to
    /// everything, but `contains_poison` is a plain structural question with
    /// no unification in it. A guard must ask this one.
    #[must_use]
    pub fn contains_poison(&self) -> bool {
        match self {
            Type::Error => true,
            _ => self.children().iter().any(|child| child.contains_poison()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_poison_is_poison() {
        assert!(Type::Error.contains_poison());
    }

    #[test]
    fn ground_types_are_clean() {
        for ty in [
            Type::Nat,
            Type::Bool,
            Type::Unit,
            Type::Void,
            Type::Prop,
            Type::String,
            Type::TyVar("T".to_string()),
        ] {
            assert!(!ty.contains_poison(), "{ty:?} should be clean");
        }
    }

    #[test]
    fn poison_in_an_argument_position_is_found() {
        let ty = Type::arrow(Type::Error, Type::Nat);
        assert!(ty.contains_poison());
    }

    #[test]
    fn poison_in_a_result_position_is_found() {
        let ty = Type::arrow(Type::Nat, Type::Error);
        assert!(ty.contains_poison());
    }

    #[test]
    fn poison_nested_several_layers_deep_is_found() {
        // ∀T. (Nat × (Nat → Error))
        let inner = Type::arrow(Type::Nat, Type::Error);
        let ty = Type::Forall("T".to_string(), Box::new(Type::product(Type::Nat, inner)));
        assert!(ty.contains_poison());
    }

    #[test]
    fn poison_behind_a_pointer_is_found() {
        assert!(Type::Ptr(Box::new(Type::Error)).contains_poison());
        assert!(Type::Ref(Box::new(Type::Error)).contains_poison());
    }

    #[test]
    fn poison_in_an_app_argument_is_found() {
        let ty = Type::App("List".to_string(), vec![Type::Error]);
        assert!(ty.contains_poison());
    }

    #[test]
    fn poison_in_an_adt_variant_payload_is_found() {
        let ty = Type::Adt(
            "Option".to_string(),
            vec![Type::Nat],
            vec![
                ("None".to_string(), Type::Unit),
                ("Some".to_string(), Type::Error),
            ],
        );
        assert!(ty.contains_poison());
    }

    #[test]
    fn poison_in_an_adt_type_argument_is_found() {
        let ty = Type::Adt(
            "Option".to_string(),
            vec![Type::Error],
            vec![("None".to_string(), Type::Unit)],
        );
        assert!(ty.contains_poison());
    }

    #[test]
    fn a_deep_clean_type_stays_clean() {
        let ty = Type::Mu(
            "α_List".to_string(),
            Box::new(Type::Sum(
                Box::new(Type::Unit),
                Box::new(Type::product(Type::Nat, Type::TyVar("α_List".to_string()))),
            )),
        );
        assert!(!ty.contains_poison());
    }
}
