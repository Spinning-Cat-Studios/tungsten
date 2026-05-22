//! Import management for module-scoped imports.
//!
//! Handles import registration, lookups, and query methods for Env.

mod lookups;

use super::{Env, ModulePath};
use crate::span::Span;

/// Information about an imported item, including where it was imported.
///
/// This tracks the origin and location of imports for:
/// 1. Error reporting: show both import locations for duplicates
/// 2. Diagnostics: "imported from both A and B" messages
/// 3. Glob expansion: only re-export items imported with `pub use`
/// 4. Canonical resolution: follow re-export chains to the original definition
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The module from which this item was imported (immediate source)
    pub source_module: ModulePath,
    /// The original name in the source module
    pub original_name: String,
    /// The span where this import was written in the source
    pub import_span: Span,
    /// Whether this import is a re-export (`pub use` vs private `use`).
    /// Only re-exports are visible through glob imports (`use foo::*`).
    pub is_reexport: bool,
    /// The visibility of the re-export statement (e.g., `pub use` → Public,
    /// `pub(crate) use` → Crate). None for private `use` statements.
    /// Used for re-export visibility capping (ADR 14.5.26c §2.3).
    pub reexport_visibility: Option<crate::ast::Visibility>,
    /// The canonical defining module (follows re-export chains).
    /// For `use parser::option::Option` where parser re-exports from core,
    /// source_module is `parser::option` but canonical_module is `core::option`.
    /// None if not yet resolved or if this is the canonical definition.
    pub canonical_module: Option<ModulePath>,
}

impl ImportInfo {
    /// Create a new ImportInfo.
    pub fn new(
        source_module: ModulePath,
        original_name: String,
        import_span: Span,
        is_reexport: bool,
    ) -> Self {
        Self {
            source_module,
            original_name,
            import_span,
            is_reexport,
            reexport_visibility: None,
            canonical_module: None,
        }
    }

    /// Create a new ImportInfo with re-export visibility.
    pub fn new_with_reexport_visibility(
        source_module: ModulePath,
        original_name: String,
        import_span: Span,
        is_reexport: bool,
        reexport_visibility: Option<crate::ast::Visibility>,
    ) -> Self {
        Self {
            source_module,
            original_name,
            import_span,
            is_reexport,
            reexport_visibility,
            canonical_module: None,
        }
    }

    /// Create a new ImportInfo with a known canonical module.
    pub fn with_canonical(
        source_module: ModulePath,
        original_name: String,
        import_span: Span,
        is_reexport: bool,
        canonical_module: ModulePath,
    ) -> Self {
        Self {
            source_module,
            original_name,
            import_span,
            is_reexport,
            reexport_visibility: None,
            canonical_module: Some(canonical_module),
        }
    }
}

/// A request to register an import in a module's scope.
///
/// Bundles the local name, source information, and re-export flag
/// that are common to all import registration functions.
pub struct ImportRequest {
    pub local_name: String,
    pub source_module: ModulePath,
    pub original_name: String,
    pub span: Span,
    pub is_reexport: bool,
    /// The visibility of the `use` statement for re-export capping.
    pub reexport_visibility: Option<crate::ast::Visibility>,
}

impl Env {
    /// Add an import for a type to a specific module's scope.
    ///
    /// `is_reexport` should be true for `pub use` imports, false for private `use`.
    /// Only re-exports are visible through glob imports.
    pub fn add_type_import(&mut self, current_module: &ModulePath, req: ImportRequest) {
        let info = ImportInfo::new_with_reexport_visibility(
            req.source_module,
            req.original_name,
            req.span,
            req.is_reexport,
            req.reexport_visibility,
        );
        self.imported_types
            .insert(req.local_name.clone(), info.clone());
        if let Some(contents) = self.modules.get_mut(current_module) {
            contents.imported_types.insert(req.local_name, info);
        }
    }

    /// Add an import for a value to a specific module's scope.
    ///
    /// `is_reexport` should be true for `pub use` imports, false for private `use`.
    /// Only re-exports are visible through glob imports.
    pub fn add_value_import(&mut self, current_module: &ModulePath, req: ImportRequest) {
        let info = ImportInfo::new_with_reexport_visibility(
            req.source_module,
            req.original_name,
            req.span,
            req.is_reexport,
            req.reexport_visibility,
        );
        self.imported_values
            .insert(req.local_name.clone(), info.clone());
        if let Some(contents) = self.modules.get_mut(current_module) {
            contents.imported_values.insert(req.local_name, info);
        }
    }

    /// Add an import for a constructor to a specific module's scope.
    ///
    /// `is_reexport` should be true for `pub use` imports, false for private `use`.
    /// Only re-exports are visible through glob imports.
    pub fn add_constructor_import(&mut self, current_module: &ModulePath, req: ImportRequest) {
        let info = ImportInfo::new_with_reexport_visibility(
            req.source_module,
            req.original_name,
            req.span,
            req.is_reexport,
            req.reexport_visibility,
        );
        self.imported_constructors
            .insert(req.local_name.clone(), info.clone());
        if let Some(contents) = self.modules.get_mut(current_module) {
            contents.imported_constructors.insert(req.local_name, info);
        }
    }
}
