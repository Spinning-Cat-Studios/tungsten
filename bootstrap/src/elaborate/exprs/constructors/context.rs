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

impl<'a> Elaborator<'a> {
    /// Look up the type definition and constructors for a constructor.
    /// Returns the constructor context with all needed info.
    ///
    /// Uses canonical lookup (ADR 31) to handle cross-module generic types.
    /// This ensures that re-exported types like `Option<T>` resolve to their
    /// original ADT definition even when imported through intermediate modules.
    pub(crate) fn get_constructor_context(
        &self,
        info: &env::ConstructorInfo,
        span: Span,
    ) -> ElabResult<ConstructorContext> {
        // ADR 31: Use canonical lookup to handle cross-module generics
        let type_def = self.env.lookup_type_canonical(&info.type_name).cloned();
        let Some(type_def) = type_def else {
            return Err(ElabError::new(
                span,
                ElabErrorKind::Other(format!(
                    "internal error: constructor's type `{}` not found",
                    info.type_name
                )),
            ));
        };

        let env::TypeDefKind::ADT(ref constructors) = type_def.kind else {
            // ADR 31: Improved error message for stub types
            let kind_desc = match type_def.kind {
                env::TypeDefKind::Stub => "a stub (type not yet elaborated)",
                env::TypeDefKind::Alias(_) => "a type alias",
                env::TypeDefKind::Record(_) => "a record type",
                env::TypeDefKind::ADT(_) => unreachable!(),
            };
            return Err(ElabError::new(
                span,
                ElabErrorKind::Other(format!(
                    "internal error: `{}` is {}, not an ADT",
                    info.type_name, kind_desc
                )),
            ));
        };

        let is_recursive = self.adt_is_recursive(&info.type_name, constructors);

        Ok(ConstructorContext {
            constructors: constructors.clone(),
            type_params: type_def.params.clone(),
            is_recursive,
        })
    }
}
