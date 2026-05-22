//! Module path resolution for use declarations.

use crate::elaborate::env::ModulePath;
use crate::elaborate::error::ElabError;
use crate::elaborate::ElabResult;
use crate::elaborate::Elaborator;

use super::{emit_relative_fallback_failure, emit_relative_fallback_warning};

impl<'a> Elaborator<'a> {
    pub(crate) fn resolve_use_module_path(
        &self,
        segments: &[String],
        use_module: &ModulePath,
        span: crate::span::Span,
    ) -> ElabResult<ModulePath> {
        if segments.is_empty() {
            return Err(ElabError::module_not_found(span, "<empty>"));
        }

        // Handle `super::` prefix - navigate up from current module
        if segments[0] == "super" {
            let parent = use_module
                .parent()
                .ok_or_else(|| ElabError::module_not_found(span, "super (no parent module)"))?;

            if segments.len() == 1 {
                // Just `super` - resolve to parent
                return Ok(parent);
            }

            // Recursively resolve the rest relative to parent
            return self.resolve_use_module_path(&segments[1..], &parent, span);
        }

        // Build the raw path from segments
        let raw_path = ModulePath::from_segments(segments);
        // Try child resolution first (current module + path)
        // This allows `use common::*` inside `ast/mod.tg` to find `ast::common`
        let child_path = use_module.join(&raw_path);
        let child_exists = self.env.has_module(&child_path);

        // Try sibling resolution (relative to use_module's parent)
        let sibling_path = use_module.parent().map(|parent| parent.join(&raw_path));
        let sibling_exists = sibling_path
            .as_ref()
            .map(|p| self.env.has_module(p))
            .unwrap_or(false);

        // Try absolute resolution (from crate root)
        let absolute_exists = self.env.has_module(&raw_path);

        // Resolution priority: child > sibling > absolute
        match (child_exists, sibling_exists, absolute_exists) {
            (true, _, _) => {
                emit_relative_fallback_warning(&raw_path, &child_path, use_module, "child");
                Ok(child_path)
            }
            (false, true, _) => {
                let sibling = sibling_path.unwrap();
                emit_relative_fallback_warning(&raw_path, &sibling, use_module, "sibling");
                Ok(sibling)
            }
            (false, false, true) => Ok(raw_path),
            (false, false, false) => {
                emit_relative_fallback_failure(&raw_path, use_module);
                Err(self.build_module_not_found_error(&raw_path, &child_path, &sibling_path, span))
            }
        }
    }

    /// Build a module-not-found error with tried paths and a suggestion.
    fn build_module_not_found_error(
        &self,
        raw_path: &ModulePath,
        child_path: &ModulePath,
        sibling_path: &Option<ModulePath>,
        span: crate::span::Span,
    ) -> ElabError {
        use crate::utils::find_best_suggestion;

        let tried_paths = [
            Some(child_path.to_string()),
            sibling_path.as_ref().map(|p| p.to_string()),
            Some(raw_path.to_string()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" or ");

        let all_modules: Vec<String> = self.env.all_module_paths().map(|p| p.to_string()).collect();

        let suggestion = find_best_suggestion(
            &raw_path.to_string(),
            all_modules.iter().map(|s| s.as_str()),
        );

        ElabError::module_not_found_with_suggestion(
            span,
            tried_paths,
            suggestion.map(|s| s.to_string()),
        )
    }
}
