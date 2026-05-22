//! Phase 1: ADT Resolution
//!
//! Resolves the ADT type from constructor patterns in match arms.

use crate::ast::{self, Pattern};
use crate::span::Span;

use super::context::AdtMatchContext;
use crate::elaborate::env::{self as elab_env, ModulePath, PathResolutionError};
use crate::elaborate::error::{ElabError, ElabErrorKind};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Resolve ADT match context from constructor patterns.
    ///
    /// Finds the first constructor pattern in the arms, resolves its path,
    /// and looks up the ADT type definition.
    pub(super) fn resolve_adt_match_context(
        &self,
        arms: &[ast::MatchArm],
        span: Span,
    ) -> ElabResult<AdtMatchContext> {
        // Find the first constructor pattern (may not be arms[0] due to catch-alls)
        let ctor_path = self.find_first_constructor_pattern(arms).ok_or_else(|| {
            ElabError::new(
                span,
                ElabErrorKind::Other("expected at least one constructor pattern".to_string()),
            )
        })?;

        // Resolve the constructor and get its ADT info
        let ctor_info = self.resolve_constructor_for_match(ctor_path)?;

        // Look up the type definition
        let type_def = self
            .env
            .lookup_type(&ctor_info.type_name)
            .cloned()
            .ok_or_else(|| {
                ElabError::new(
                    span,
                    ElabErrorKind::Other(format!(
                        "internal error: type `{}` not found",
                        ctor_info.type_name
                    )),
                )
            })?;

        // Extract constructors from ADT
        let constructors = match &type_def.kind {
            elab_env::TypeDefKind::ADT(ctors) => ctors.clone(),
            _ => {
                return Err(ElabError::new(
                    span,
                    ElabErrorKind::Other(format!("`{}` is not an ADT", ctor_info.type_name)),
                ))
            }
        };

        // Check if it's recursive (self-recursive or in a mutual recursion group)
        let is_recursive = self.adt_is_recursive(&ctor_info.type_name, &constructors);

        Ok(AdtMatchContext {
            type_def,
            constructors,
            is_recursive,
        })
    }

    /// Find the first constructor pattern in a list of match arms.
    pub(super) fn find_first_constructor_pattern<'b>(
        &self,
        arms: &'b [ast::MatchArm],
    ) -> Option<&'b ast::Path> {
        arms.iter().find_map(|arm| {
            if let Pattern::Constructor(ref path, _, _) = arm.pattern {
                Some(path)
            } else {
                None
            }
        })
    }

    /// Resolve a constructor path for pattern matching.
    ///
    /// Checks module visibility and resolves the constructor info.
    pub(super) fn resolve_constructor_for_match(
        &self,
        path: &ast::Path,
    ) -> ElabResult<elab_env::ConstructorInfo> {
        let name = path.item_name();

        // Check module visibility for qualified paths
        if !path.is_simple() {
            let module_path = ModulePath::new(
                path.module_segments()
                    .iter()
                    .map(|s| s.name.clone())
                    .collect(),
            );
            if !self
                .env
                .is_module_accessible(&module_path, &self.current_module, true)
            {
                return Err(ElabError::private_module(
                    path.span,
                    module_path.to_string(),
                    self.current_module.to_string(),
                ));
            }
        }

        match self
            .env
            .resolve_constructor_path(path, &self.current_module)
        {
            Ok(Some(info)) => {
                let info = info.clone();
                // Check constructor visibility (ADR 14.5.26c AC3)
                if !self
                    .env
                    .is_constructor_accessible(&info, &self.current_module, true)
                {
                    if let Some(item_module) = self.env.get_item_module(&info.type_name) {
                        return Err(ElabError::private_item(
                            path.span,
                            &name.name,
                            "constructor",
                            item_module.to_string(),
                            self.current_module.to_string(),
                        ));
                    }
                }
                Ok(info)
            }
            Ok(None) => Err(self.undefined_constructor_error(name.span, &name.name)),
            Err(PathResolutionError::ModuleNotFound(module)) => {
                Err(ElabError::module_not_found(path.span, module.to_string()))
            }
            Err(PathResolutionError::ItemNotFound { module, item }) => Err(
                ElabError::item_not_in_module(path.span, module.to_string(), item),
            ),
        }
    }
}
