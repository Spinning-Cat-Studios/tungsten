//! What the elaborator refuses to hand onward (ADR 7.8.26d §2.2).
//!
//! `Type::Error` is poison: it compares equal to everything, which is the
//! whole point at the compare boundary and a disaster anywhere else. The two
//! refusals here are the elaborator's half of D3's "poison never succeeds"
//! rule — the third lives at codegen entry
//! (`tungsten_codegen::types::lowering`).
//!
//! - [`Elaborator::admit_core_def`] keeps a poisoned definition out of Core
//!   and CIR, so a body that elaborated only *because* its signature was
//!   poisoned never becomes a `CoreDef`.
//! - [`first_poisoned_export`] keeps poison out of the elaboration cache,
//!   where it would be **durable**: a signature that is `Type::Error` only
//!   because *this* run failed, reloaded by a later run as a real type.
//!
//! The cache route is closed on `main` already — `ModuleExports` is built
//! only inside `if self.elaborator.errors.is_empty()`, so a failing run
//! writes no exports. The check pins it closed against a future decoupling of
//! export construction from error state, which is precisely the change that
//! would open it silently.

use super::{CoreDef, ModuleExports};
use crate::elaborate::env::TypeDefKind;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::Elaborator;

impl<'a> Elaborator<'a> {
    /// Admit an elaborated definition to the Core output, or refuse it as poisoned.
    ///
    /// Returns `None` for a definition whose type carries `Type::Error`. Such
    /// a definition is not a success: it type-checked against a poisoned
    /// binding, so its Core term is meaningless and lowering it would be a
    /// silent miscompile.
    ///
    /// A poisoned definition with **no error recorded** means a producer
    /// fired without reporting, which would turn this refusal into a silently
    /// dropped definition — a wrong-and-quiet outcome worse than the cascade.
    /// That case records its own diagnostic so the run still fails loudly.
    pub(crate) fn admit_core_def(&mut self, def: CoreDef) -> Option<CoreDef> {
        if !def.ty.contains_poison() {
            return Some(def);
        }
        if self.errors.is_empty() {
            self.record_error(ElabError::internal(
                def.span,
                format!(
                    "definition `{}` elaborated to a poisoned type \
                     with no error recorded — a failed elaboration poisoned an \
                     environment entry silently (ADR 7.8.26d §2.2)",
                    def.name
                ),
            ));
        }
        None
    }
}

/// The name of the first poisoned entry in `exports`, if any.
///
/// Scans value signatures and type bodies alike: a record field or ADT
/// constructor field is as durable in the cache as a whole signature.
#[must_use]
pub fn first_poisoned_export(exports: &ModuleExports) -> Option<String> {
    if let Some((name, _)) = exports
        .values
        .iter()
        .find(|(_, def)| def.ty.contains_poison())
    {
        return Some(name.clone());
    }
    exports
        .types
        .iter()
        .find(|(_, def)| {
            def.encoded_type
                .as_ref()
                .is_some_and(tungsten_core::Type::contains_poison)
                || match &def.kind {
                    TypeDefKind::Alias(ty) => ty.contains_poison(),
                    TypeDefKind::ADT(ctors) => ctors
                        .iter()
                        .any(|c| c.fields.iter().any(tungsten_core::Type::contains_poison)),
                    TypeDefKind::Record(fields) => {
                        fields.iter().any(|(_, ty)| ty.contains_poison())
                    }
                    TypeDefKind::Stub => false,
                }
        })
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests;
