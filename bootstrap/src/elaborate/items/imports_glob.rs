//! Glob Import Processing
//!
//! Handles `use foo::*;` imports, importing all public items from a module.
//! Split from `imports.rs` to reduce file complexity.

use crate::ast::{self, Visibility};

use crate::elaborate::env::{ImportRequest, ModulePath};
use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Process a glob import (`use foo::*;`), importing all public items from a module.
    ///
    /// This imports:
    /// - All types, values, and constructors defined in the module with pub visibility
    /// - All types, values, and constructors defined with pub(crate) visibility (within same crate)
    /// - All public re-exports from the module (items imported with `pub use`)
    ///
    /// The `is_reexport` parameter indicates whether the glob import itself is public (`pub use foo::*;`).
    /// This determines whether items imported via this glob will be re-exported to other modules.
    pub(super) fn process_use_glob(
        &mut self,
        prefix: &ast::Path,
        span: crate::span::Span,
        use_module: &ModulePath,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<()> {
        // Glob needs at least one segment (the module to import from)
        if prefix.segments.is_empty() {
            return Err(ElabError::unresolved_import(span, "*"));
        }

        // Get the module segments
        let module_segments: Vec<String> = prefix.segments.iter().map(|s| s.name.clone()).collect();

        // Resolve the module path relative to the use_module
        let source_module = self.resolve_use_module_path(&module_segments, use_module, span)?;

        // Check module visibility
        if !self
            .env
            .is_module_accessible(&source_module, use_module, true)
        {
            return Err(ElabError::private_module(
                span,
                source_module.to_string(),
                use_module.to_string(),
            ));
        }

        // Get the module contents
        let contents = match self.env.get_module(&source_module) {
            Some(c) => c.clone(),
            None => return Err(ElabError::module_not_found(span, source_module.to_string())),
        };

        // Import direct definitions (types, values, constructors)
        self.glob_import_direct_items(&contents, &source_module, use_module, span, is_reexport)?;

        // Import public re-exports
        self.glob_import_reexports(&contents, use_module, span, is_reexport)?;

        Ok(())
    }

    /// Import all direct definitions (types, values, constructors) from a module.
    fn glob_import_direct_items(
        &mut self,
        contents: &crate::elaborate::env::ModuleContents,
        source_module: &ModulePath,
        use_module: &ModulePath,
        span: crate::span::Span,
        is_reexport: bool,
    ) -> ElabResult<()> {
        for type_name in &contents.types {
            let visibility = contents
                .type_visibility
                .get(type_name)
                .copied()
                .unwrap_or(Visibility::Public);
            if self.is_glob_importable(visibility, source_module, use_module) {
                self.try_glob_import_type(
                    type_name.clone(),
                    source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        for value_name in &contents.values {
            let visibility = contents
                .value_visibility
                .get(value_name)
                .copied()
                .unwrap_or(Visibility::Public);
            if self.is_glob_importable(visibility, source_module, use_module) {
                self.try_glob_import_value(
                    value_name.clone(),
                    source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        for ctor_name in &contents.constructors {
            let visibility = contents
                .constructor_visibility
                .get(ctor_name)
                .copied()
                .unwrap_or(Visibility::Public);
            if self.is_glob_importable(visibility, source_module, use_module) {
                self.try_glob_import_constructor(
                    ctor_name.clone(),
                    source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        Ok(())
    }

    /// Import public re-exports from a module's imported items.
    fn glob_import_reexports(
        &mut self,
        contents: &crate::elaborate::env::ModuleContents,
        use_module: &ModulePath,
        span: crate::span::Span,
        is_reexport: bool,
    ) -> ElabResult<()> {
        for (local_name, import_info) in &contents.imported_types {
            if import_info.is_reexport {
                self.try_glob_import_type(
                    local_name.clone(),
                    &import_info.source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        for (local_name, import_info) in &contents.imported_values {
            if import_info.is_reexport {
                self.try_glob_import_value(
                    local_name.clone(),
                    &import_info.source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        for (local_name, import_info) in &contents.imported_constructors {
            if import_info.is_reexport {
                self.try_glob_import_constructor(
                    local_name.clone(),
                    &import_info.source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        Ok(())
    }

    /// Check if an item with given visibility can be glob-imported.
    ///
    /// Glob imports only include:
    /// - `pub` items (public to everyone)
    /// - `pub(crate)` items (when importing within the same crate)
    fn is_glob_importable(
        &self,
        visibility: Visibility,
        _source_module: &ModulePath,
        _use_module: &ModulePath,
    ) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Crate => true, // Within same crate (always true for now)
            Visibility::Private => false,
        }
    }

    /// Try to import a type via glob, checking for conflicts.
    ///
    /// `is_reexport` indicates whether this glob import is itself a re-export (`pub use foo::*;`).
    fn try_glob_import_type(
        &mut self,
        name: String,
        source_module: &ModulePath,
        use_module: &ModulePath,
        span: crate::span::Span,
        is_reexport: bool,
    ) -> ElabResult<()> {
        // Check if already imported
        if let Some(existing) = self.env.lookup_type_import(use_module, &name) {
            // Conflict: same name imported from different modules
            if &existing.source_module != source_module {
                return Err(ElabError::glob_conflict(
                    span,
                    name,
                    existing.source_module.to_string(),
                    source_module.to_string(),
                ));
            }
            // Same module - already imported, skip
            return Ok(());
        }

        // Check if name conflicts with a local definition
        let has_type = self.env.has_type_in_module(use_module, &name);
        if has_type {
            // Local definition shadows glob import - skip silently
            return Ok(());
        }

        self.env.add_type_import(
            use_module,
            ImportRequest {
                local_name: name.clone(),
                source_module: source_module.clone(),
                original_name: name,
                span,
                is_reexport,
                reexport_visibility: None,
            },
        );
        Ok(())
    }

    /// Try to import a value via glob, checking for conflicts.
    ///
    /// `is_reexport` indicates whether this glob import is itself a re-export (`pub use foo::*;`).
    fn try_glob_import_value(
        &mut self,
        name: String,
        source_module: &ModulePath,
        use_module: &ModulePath,
        span: crate::span::Span,
        is_reexport: bool,
    ) -> ElabResult<()> {
        if let Some(existing) = self.env.lookup_value_import(use_module, &name) {
            if &existing.source_module != source_module {
                return Err(ElabError::glob_conflict(
                    span,
                    name,
                    existing.source_module.to_string(),
                    source_module.to_string(),
                ));
            }
            return Ok(());
        }

        if self.env.has_value_in_module(use_module, &name) {
            return Ok(());
        }

        self.env.add_value_import(
            use_module,
            ImportRequest {
                local_name: name.clone(),
                source_module: source_module.clone(),
                original_name: name,
                span,
                is_reexport,
                reexport_visibility: None,
            },
        );
        Ok(())
    }

    /// Try to import a constructor via glob, checking for conflicts.
    ///
    /// `is_reexport` indicates whether this glob import is itself a re-export (`pub use foo::*;`).
    fn try_glob_import_constructor(
        &mut self,
        name: String,
        source_module: &ModulePath,
        use_module: &ModulePath,
        span: crate::span::Span,
        is_reexport: bool,
    ) -> ElabResult<()> {
        if let Some(existing) = self.env.lookup_constructor_import(use_module, &name) {
            if &existing.source_module != source_module {
                return Err(ElabError::glob_conflict(
                    span,
                    name,
                    existing.source_module.to_string(),
                    source_module.to_string(),
                ));
            }
            return Ok(());
        }

        if self.env.has_constructor_in_module(use_module, &name) {
            return Ok(());
        }

        self.env.add_constructor_import(
            use_module,
            ImportRequest {
                local_name: name.clone(),
                source_module: source_module.clone(),
                original_name: name,
                span,
                is_reexport,
                reexport_visibility: None,
            },
        );
        Ok(())
    }
}
