//! Constructor context lookup.

use crate::elaborate::env;
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};
use crate::span::Span;

/// Context for constructor elaboration, containing all needed type info.
pub(crate) struct ConstructorContext {
    /// The constructors of the ADT
    pub constructors: Vec<env::Constructor>,
    /// Type parameters of the ADT
    pub type_params: Vec<String>,
    /// Whether the ADT is recursive
    pub is_recursive: bool,
}

/// What a constructor's parent type resolved to.
pub(crate) enum ConstructorParent {
    /// A healthy ADT, with everything a constructor site needs.
    Adt(ConstructorContext),
    /// The parent's own body failed to elaborate (ADR 15.8.26d D2). Its
    /// stub carries no constructor list and the fault is already reported
    /// at the type's span, so the site passes poison through: arguments
    /// check against `Type::Error` and the value is a hole.
    Poisoned,
}

impl<'a> Elaborator<'a> {
    /// Look up the type definition and constructors for a constructor.
    /// Returns the constructor context with all needed info, or the
    /// poison verdict when the parent type failed.
    ///
    /// Uses canonical lookup (ADR 31) to handle cross-module generic types.
    /// This ensures that re-exported types like `Option<T>` resolve to their
    /// original ADT definition even when imported through intermediate modules.
    pub(crate) fn get_constructor_context(
        &self,
        info: &env::ConstructorInfo,
        span: Span,
    ) -> ElabResult<ConstructorParent> {
        // ADR 31: Use canonical lookup to handle cross-module generics
        let type_def = self.env.lookup_type_canonical(&info.type_name).cloned();
        let Some(type_def) = type_def else {
            return Err(ElabError::internal(
                span,
                format!("constructor's type `{}` not found", info.type_name),
            ));
        };

        if type_def.is_poison() {
            return Ok(ConstructorParent::Poisoned);
        }

        let env::TypeDefKind::ADT(ref constructors) = type_def.kind else {
            // ADR 31: Improved error message for stub types
            let kind_desc = match type_def.kind {
                env::TypeDefKind::Stub => "a stub (type not yet elaborated)",
                env::TypeDefKind::Alias(_) => "a type alias",
                env::TypeDefKind::Record(_) => "a record type",
                env::TypeDefKind::ADT(_) => unreachable!(),
            };
            return Err(ElabError::internal(
                span,
                format!("`{}` is {}, not an ADT", info.type_name, kind_desc),
            ));
        };

        let is_recursive = self.adt_is_recursive(&info.type_name, constructors);

        Ok(ConstructorParent::Adt(ConstructorContext {
            constructors: constructors.clone(),
            type_params: type_def.params.clone(),
            is_recursive,
        }))
    }
}
