//! TyVar-related operations on `Type` — walking and normalization.
//!
//! Extracted from `types/mod.rs` (ADR 10.5.26d) to keep file sizes under
//! the 400-line limit.

use super::Type;
use std::collections::HashSet;

impl Type {
    /// Walk the type tree and return `true` if any TyVar satisfies `predicate`.
    ///
    /// The predicate receives the TyVar name. This is the shared backbone for
    /// `has_mono_blocking_tyvar`.
    #[must_use]
    pub fn any_tyvar<F: Fn(&str) -> bool>(&self, predicate: &F) -> bool {
        match self {
            Type::TyVar(name) => predicate(name),
            Type::Arrow(a, b) | Type::Product(a, b) | Type::Sum(a, b) => {
                a.any_tyvar(predicate) || b.any_tyvar(predicate)
            }
            Type::Forall(_, inner) | Type::Mu(_, inner) | Type::Ptr(inner) | Type::Ref(inner) => {
                inner.any_tyvar(predicate)
            }
            Type::Eq(t, _, _) => t.any_tyvar(predicate),
            Type::App(_, args) => args.iter().any(|a| a.any_tyvar(predicate)),
            Type::Adt(_, type_args, variants) => {
                type_args.iter().any(|a| a.any_tyvar(predicate))
                    || variants.iter().any(|(_, t)| t.any_tyvar(predicate))
            }
            Type::Nat
            | Type::Bool
            | Type::String
            | Type::Unit
            | Type::Void
            | Type::Prop
            | Type::Error => false,
        }
    }

    /// Check whether a type contains any TyVar that blocks monomorphization.
    ///
    /// A TyVar is "mono-blocking" if it is NOT:
    /// - `@`-prefixed (Phase 1c cross-reference to a concrete type)
    /// - `α_`-prefixed (Mu-bound variable in recursive type encoding)
    /// - A known concrete type name (ADT or record registered during elaboration)
    ///
    /// Type aliases are NOT included in `concrete_type_names` — they are expanded
    /// during elaboration and do not appear as bare `TyVar` in Core IR bodies.
    ///
    /// This is the single canonical predicate used by both mono discovery
    /// (`bootstrap::compile::mono::discovery`) and codegen
    /// (`tungsten_codegen::codegen::exec::polymorphism`). See ADR 13.5.26a.
    #[must_use]
    pub fn has_mono_blocking_tyvar(&self, concrete_type_names: &HashSet<String>) -> bool {
        self.any_tyvar(&|name| {
            !name.starts_with('@') && !name.starts_with("α_") && !concrete_type_names.contains(name)
        })
    }

    /// Strip `@` prefixes from all TyVars in a type tree.
    ///
    /// `@`-prefixed TyVars are Phase 1c artifacts referencing concrete named
    /// types (e.g., `@Token` → `Token`). Stripping normalizes types so that
    /// `@Token` and `Token` produce identical mono keys.
    #[must_use]
    pub fn strip_tyvar_at_prefix(&self) -> Type {
        match self {
            Type::TyVar(name) => {
                if let Some(stripped) = name.strip_prefix('@') {
                    Type::TyVar(stripped.to_string())
                } else {
                    self.clone()
                }
            }
            Type::Arrow(a, b) => Type::Arrow(
                Box::new(a.strip_tyvar_at_prefix()),
                Box::new(b.strip_tyvar_at_prefix()),
            ),
            Type::Product(a, b) => Type::Product(
                Box::new(a.strip_tyvar_at_prefix()),
                Box::new(b.strip_tyvar_at_prefix()),
            ),
            Type::Sum(a, b) => Type::Sum(
                Box::new(a.strip_tyvar_at_prefix()),
                Box::new(b.strip_tyvar_at_prefix()),
            ),
            Type::Forall(v, inner) => {
                Type::Forall(v.clone(), Box::new(inner.strip_tyvar_at_prefix()))
            }
            Type::Mu(v, inner) => Type::Mu(v.clone(), Box::new(inner.strip_tyvar_at_prefix())),
            Type::Ptr(inner) => Type::Ptr(Box::new(inner.strip_tyvar_at_prefix())),
            Type::Ref(inner) => Type::Ref(Box::new(inner.strip_tyvar_at_prefix())),
            Type::Eq(t, a, b) => {
                Type::Eq(Box::new(t.strip_tyvar_at_prefix()), a.clone(), b.clone())
            }
            Type::App(name, args) => Type::App(
                name.clone(),
                args.iter()
                    .map(super::Type::strip_tyvar_at_prefix)
                    .collect(),
            ),
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args
                    .iter()
                    .map(super::Type::strip_tyvar_at_prefix)
                    .collect(),
                variants
                    .iter()
                    .map(|(n, t)| (n.clone(), t.strip_tyvar_at_prefix()))
                    .collect(),
            ),
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn concrete(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn abstract_tyvar_blocks() {
        let ty = Type::TyVar("T".into());
        assert!(ty.has_mono_blocking_tyvar(&HashSet::new()));
    }

    #[test]
    fn concrete_tyvar_does_not_block() {
        let ty = Type::TyVar("Token".into());
        assert!(!ty.has_mono_blocking_tyvar(&concrete(&["Token"])));
    }

    #[test]
    fn at_prefixed_tyvar_does_not_block() {
        let ty = Type::TyVar("@Token".into());
        assert!(!ty.has_mono_blocking_tyvar(&HashSet::new()));
    }

    #[test]
    fn alpha_prefixed_tyvar_does_not_block() {
        let ty = Type::TyVar("α_List".into());
        assert!(!ty.has_mono_blocking_tyvar(&HashSet::new()));
    }

    #[test]
    fn mixed_concrete_abstract_compound_blocks() {
        let ty = Type::product(Type::TyVar("Token".into()), Type::TyVar("T".into()));
        assert!(ty.has_mono_blocking_tyvar(&concrete(&["Token"])));
    }

    #[test]
    fn all_concrete_compound_does_not_block() {
        let ty = Type::product(Type::TyVar("Token".into()), Type::TyVar("Span".into()));
        assert!(!ty.has_mono_blocking_tyvar(&concrete(&["Token", "Span"])));
    }

    #[test]
    fn nat_does_not_block() {
        assert!(!Type::Nat.has_mono_blocking_tyvar(&HashSet::new()));
    }

    #[test]
    fn nested_mu_with_concrete_tyvar_does_not_block() {
        // Mu("α_List", Sum(Unit, Product(TyVar("Token"), TyVar("α_List"))))
        // α_List is Mu-bound, Token is concrete → should not block
        let ty = Type::Mu(
            "α_List".into(),
            Box::new(Type::Sum(
                Box::new(Type::Unit),
                Box::new(Type::Product(
                    Box::new(Type::TyVar("Token".into())),
                    Box::new(Type::TyVar("α_List".into())),
                )),
            )),
        );
        assert!(!ty.has_mono_blocking_tyvar(&concrete(&["Token"])));
    }

    #[test]
    fn adt_variant_field_with_abstract_tyvar_blocks() {
        // Adt("Foo", [], [("A", TyVar("T"))]) where T is not concrete → blocks
        let ty = Type::Adt(
            "Foo".into(),
            vec![],
            vec![("A".into(), Type::TyVar("T".into()))],
        );
        assert!(ty.has_mono_blocking_tyvar(&HashSet::new()));
    }
}
