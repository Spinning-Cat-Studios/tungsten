//! Unified structural walker for the twin type-reference resolvers
//! (ADR 23.7.26a).
//!
//! Before this module, the Phase-1d resolver (`resolve_tyvars_in_type_impl`,
//! `elaborate/resolve_tyvars.rs`) and the encoding-path resolver
//! (`resolve_type_references_impl`, `types/resolve_refs.rs`) were two
//! hand-synced copies of the same ten-arm structural traversal, diverging
//! only in the name-bearing arms — and that divergence is load-bearing
//! (the `@`-strip asymmetry ADR 22.7.26d's record-body freeze turned on).
//! [`Elaborator::walk_type_refs`] states the shared traversal once, driven
//! by [`Type::map_children`], and delegates the name arms to an explicit
//! [`TypeRefStrategy`], so the asymmetry is a named choice instead of a
//! diff between two files.

use std::collections::HashSet;

use tungsten_core::Type;

use crate::elaborate::types::resolve_refs::AppResolveMode;
use crate::elaborate::Elaborator;

/// The name-arm strategy of [`Elaborator::walk_type_refs`] — the ONLY
/// behavioural difference between the two type-reference walkers.
///
/// | arm | `Deferred` | `Encoding` |
/// |-----|-----------|------------|
/// | `TyVar` in-stack guard | `@`-stripped name | bare AND stripped name |
/// | free `TyVar` | `resolve_tyvar_definition` — strips `@` and resolves a *deferred* `TyVar("@X")` | `resolve_type_ref_tyvar` — bare-name lookup only; an `@`-ref embeds unresolved |
/// | free `App` | `resolve_tyvars_app` — pre-inserts the name into the stack around the whole expansion (including ADT encode) | `resolve_app_to_encoding` — no pre-insert for ADTs (`encode_adt_type_impl` manages its own stack entry) |
///
/// Everything else — every structural arm — is shared via
/// [`Type::map_children`] in [`Elaborator::walk_type_refs`].
///
/// Each variant is named for what it *does*, not for where it runs:
/// `Deferred` resolves deferred references, `Encoding` inlines during
/// encoding.
#[derive(Clone, Copy)]
pub(crate) enum TypeRefStrategy {
    /// Resolve deferred `@`-prefixed cross-references now that all types are
    /// elaborated: strip the `@` and resolve the deferred `TyVar`. This is the
    /// behaviour of the Phase-1d pass `resolve_deferred_type_references`
    /// (ADR 13.4.26c §2).
    Deferred,
    /// Inline already-elaborated referents while encoding an ADT/record body
    /// (the Phase-1e encoding path). `@`-refs are a Phase-1c/1d-era spelling
    /// and are deliberately NOT resolved here — bare-name lookup only
    /// (ADR 22.7.26d).
    Encoding,
}

impl TypeRefStrategy {
    /// The `TyVar` in-stack guard — the one *guard* that differs between the
    /// walkers. `Deferred` checks the `@`-stripped name; `Encoding` checks
    /// both the bare and the stripped spelling. (The `App` guard is
    /// byte-identical across both and lives in `walk_type_refs` itself.)
    fn tyvar_in_stack(self, expansion_stack: &HashSet<String>, name: &str) -> bool {
        let stripped = Elaborator::strip_named_prefix(name);
        match self {
            TypeRefStrategy::Deferred => expansion_stack.contains(stripped),
            TypeRefStrategy::Encoding => {
                expansion_stack.contains(name) || expansion_stack.contains(stripped)
            }
        }
    }

    /// Resolve a free `TyVar` reference — the `@`-strip-vs-bare difference
    /// (row 2 of the doc table above).
    fn resolve_tyvar(
        self,
        elab: &mut Elaborator,
        name: &str,
        original: &Type,
        expansion_stack: &mut HashSet<String>,
    ) -> Type {
        match self {
            TypeRefStrategy::Deferred => {
                elab.resolve_tyvar_definition(name, original, expansion_stack)
            }
            TypeRefStrategy::Encoding => {
                elab.resolve_type_ref_tyvar(name, original, expansion_stack)
            }
        }
    }

    /// Resolve a free `App` head, its args already resolved by the walker —
    /// the pre-insert-vs-not difference (row 3 of the doc table above).
    fn resolve_app(
        self,
        elab: &mut Elaborator,
        name: &str,
        resolved_args: Vec<Type>,
        expansion_stack: &mut HashSet<String>,
    ) -> Type {
        match self {
            TypeRefStrategy::Deferred => {
                elab.resolve_tyvars_app(name, resolved_args, expansion_stack)
            }
            TypeRefStrategy::Encoding => elab.resolve_app_to_encoding(
                name,
                resolved_args,
                expansion_stack,
                AppResolveMode::TypeRefs,
            ),
        }
    }
}

impl<'a> Elaborator<'a> {
    /// Resolve type references in `ty`, with cycle detection via
    /// `expansion_stack`, delegating the name-bearing arms to `strategy`.
    ///
    /// The single structural traversal behind both
    /// `resolve_tyvars_in_type_impl` (Deferred-TyVar Resolution) and
    /// `resolve_type_references_impl` (encoding path). Every arm other than
    /// the name-bearing ones is the structural default: the final arm is
    /// `Type::map_children` — deliberately NOT a `_ => ty.clone()` — so a
    /// future `Type` variant inherits correct recursion in both walkers with
    /// no edit here (ADR 23.7.26a §5).
    pub(crate) fn walk_type_refs(
        &mut self,
        ty: &Type,
        strategy: TypeRefStrategy,
        expansion_stack: &mut HashSet<String>,
    ) -> Type {
        match ty {
            Type::TyVar(name) if !strategy.tyvar_in_stack(expansion_stack, name) => {
                strategy.resolve_tyvar(self, name, ty, expansion_stack)
            }
            // In the expansion stack (cycle) or a genuinely bound variable.
            Type::TyVar(_) => ty.clone(),

            // Free type application: resolve args first, then let the
            // strategy expand the head. Args are resolved BEFORE the head
            // name enters the stack, matching both original walkers.
            Type::App(name, args) if !expansion_stack.contains(name) => {
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|arg| self.walk_type_refs(arg, strategy, expansion_stack))
                    .collect();
                strategy.resolve_app(self, name, resolved_args, expansion_stack)
            }

            // Everything else — including the cycle-detected `App`
            // fallthrough, whose args still get resolved — is uniform
            // structural recursion.
            _ => ty.map_children(|child| self.walk_type_refs(child, strategy, expansion_stack)),
        }
    }
}
