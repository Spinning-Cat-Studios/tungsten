//! The E0064 diagnostic: what to SAY when the inner-μ peel cannot flatten.
//!
//! Split from `mod.rs` so that file answers one question — how the chain is
//! peeled — and this one answers the other: what a reader is told when it
//! cannot be. The two have genuinely different shapes. Peeling is a loop with
//! a termination argument; this is a search over a type *definition*, needed
//! because the encoding has erased the very thing the reader needs.
//!
//! **Why the recovery exists at all.** By the time the peel fails, `Rose` has
//! encoded to `μα_Rose. α_Rose` — no trace of the `Wrap` the recursion was
//! nested under. The generic is the one part of the diagnostic a reader cannot
//! re-derive from their own source without already knowing what "nested"
//! means, and the tool that would otherwise show the chain
//! (`info type type-encoding`) is blocked by this very gate. So it is
//! recovered from the definition, here.

use tungsten_core::Type;

use super::{UnflattenedMu, UnflattenedMuCause};
use crate::elaborate::env::TypeDefKind;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::Elaborator;
use crate::span::Span;

impl<'a> Elaborator<'a> {
    /// Build the E0064 rejection for a μ chain that would not flatten
    /// (ADR 11.8.26c §2.2).
    ///
    /// Lives beside [`UnflattenedMu`] rather than at any one call site because
    /// all three consumers — match elaboration, constructor injection and
    /// pattern unwrapping — peel the same chain and must word the same
    /// rejection identically.
    ///
    /// The note differs by cause: a repeated binder is a *nested inductive
    /// family*, a language-level Phase-1 restriction the user can work around;
    /// a missing cached encoding is an internal inconsistency, and saying so
    /// keeps a compiler bug from reading as a source error.
    pub(in crate::elaborate) fn nested_family_error(
        &self,
        unflattened: &UnflattenedMu,
        span: Span,
    ) -> ElabError {
        let type_name = unflattened.type_name().to_string();
        let nested_under = self.generic_nesting_recursion(&type_name);
        let in_place_of = nested_under
            .as_ref()
            .map(|generic| format!(", in place of `{generic}<{type_name}>`"))
            .unwrap_or_default();
        let note = match unflattened.cause {
            UnflattenedMuCause::BinderRepeats => {
                format!("break the nesting with a non-generic intermediate type{in_place_of}")
            }
            UnflattenedMuCause::MissingEncoding => format!(
                "the environment holds no cached encoding for `{type_name}`; \
                 this is a compiler inconsistency, not a source error"
            ),
        };
        ElabError::new(
            span,
            ElabErrorKind::NestedRecursiveFamily {
                type_name,
                binder: unflattened.binder.clone(),
                nested_under,
            },
        )
        .with_note(note)
    }

    /// The generic type a recursive occurrence of `type_name` sits under, if
    /// any — `Wrap`, for `type Rose = Node(Wrap<Rose>)`.
    ///
    /// The encoding has erased this by the time the peel fails (`Rose` encodes
    /// to `μα_Rose. α_Rose`, with no trace of `Wrap`), but it is the one part
    /// of the diagnostic the reader cannot re-derive from their own source
    /// without knowing what "nested" means. Recovered from the *definition*
    /// rather than the encoding for exactly that reason.
    pub(in crate::elaborate) fn generic_nesting_recursion(
        &self,
        type_name: &str,
    ) -> Option<String> {
        let type_def = self.env.lookup_type(type_name)?;
        let field_types: Vec<&Type> = match &type_def.kind {
            TypeDefKind::ADT(ctors) => ctors.iter().flat_map(|c| c.fields.iter()).collect(),
            TypeDefKind::Record(fields) => fields.iter().map(|(_, ty)| ty).collect(),
            TypeDefKind::Alias(_) | TypeDefKind::Stub => return None,
        };
        field_types
            .into_iter()
            .find_map(|field| generic_wrapping(field, type_name))
    }
}

/// Innermost-first search for a `Type::App(generic, args)` one of whose
/// arguments mentions `type_name`.
///
/// Innermost-first so `Wrap<Box<Rose>>` names `Box` — the wrapper actually
/// holding the recursive occurrence — rather than the outer one.
pub(super) fn generic_wrapping(ty: &Type, type_name: &str) -> Option<String> {
    match ty {
        Type::App(generic, args) => args
            .iter()
            .find_map(|arg| generic_wrapping(arg, type_name))
            .or_else(|| {
                args.iter()
                    .any(|arg| mentions_type(arg, type_name))
                    .then(|| generic.clone())
            }),
        Type::Product(a, b) | Type::Sum(a, b) | Type::Arrow(a, b) => {
            generic_wrapping(a, type_name).or_else(|| generic_wrapping(b, type_name))
        }
        Type::Ptr(inner) | Type::Ref(inner) | Type::Mu(_, inner) | Type::Forall(_, inner) => {
            generic_wrapping(inner, type_name)
        }
        _ => None,
    }
}

/// Whether `ty` refers to `type_name`, under either the source spelling or the
/// `α_`-prefixed μ-binder spelling the encoder substitutes for it.
pub(super) fn mentions_type(ty: &Type, type_name: &str) -> bool {
    match ty {
        Type::TyVar(name) => name == type_name || name.strip_prefix("α_") == Some(type_name),
        Type::App(name, args) => {
            name == type_name || args.iter().any(|arg| mentions_type(arg, type_name))
        }
        Type::Product(a, b) | Type::Sum(a, b) | Type::Arrow(a, b) => {
            mentions_type(a, type_name) || mentions_type(b, type_name)
        }
        Type::Ptr(inner) | Type::Ref(inner) | Type::Mu(_, inner) | Type::Forall(_, inner) => {
            mentions_type(inner, type_name)
        }
        _ => false,
    }
}
