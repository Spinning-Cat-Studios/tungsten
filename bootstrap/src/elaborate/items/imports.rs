//! Use Declaration Processing
//!
//! Handles processing of `use` declarations, resolving module paths
//! and adding imports to the environment.

use crate::ast::{self, ExpandedUseTree, UseDecl, Visibility};

use crate::elaborate::env::ModulePath;
use crate::elaborate::error::ElabError;
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Process a `use` declaration with known item index for file provenance.
    ///
    /// The index is used to look up the source file from `item_index_to_file`,
    /// which enables correct module resolution when different files have
    /// items at the same byte offsets.
    pub(crate) fn process_use_decl_with_index(
        &mut self,
        use_decl: &UseDecl,
        item_index: usize,
    ) -> ElabResult<()> {
        // Look up the source file using the item index
        let use_module = if let Some(file_path) = self.env.get_item_file(item_index) {
            // Use file-keyed lookup for precise disambiguation
            self.env
                .get_use_statement_module_by_file(file_path, use_decl.span.start)
                .cloned()
                .unwrap_or_else(ModulePath::root)
        } else {
            // Fallback to span-based lookup (may have collisions)
            self.env
                .get_use_statement_module(use_decl.span.start, use_decl.span.end)
                .cloned()
                .unwrap_or_else(ModulePath::root)
        };

        // Set current_module for error reporting
        self.current_module = use_module.clone();

        // Determine if this is a re-export (`pub use` vs private `use`)
        let is_reexport = matches!(use_decl.visibility, Visibility::Public | Visibility::Crate);

        // Expand the use tree into paths or glob imports
        let expanded = use_decl.tree.expand();
        match expanded {
            ExpandedUseTree::Paths(paths) => {
                for path in paths {
                    self.process_use_path(&path, &use_module, is_reexport)?;
                }
            }
            ExpandedUseTree::Glob { prefix, span } => {
                self.process_use_glob(&prefix, span, &use_module, is_reexport)?;
            }
        }

        Ok(())
    }

    /// Process a `use` declaration, adding imports to the environment.
    ///
    /// This should be called after the collection pass, once all definitions
    /// are known. It processes each expanded path in the use tree and adds
    /// appropriate imports for types, values, and constructors.
    ///
    /// Supports both regular imports (`use foo::bar;`) and glob imports (`use foo::*;`).
    ///
    /// NOTE: Prefer `process_use_decl_with_index` when the item index is known,
    /// as it provides better disambiguation for multi-file compilation.
    #[allow(dead_code)]
    pub(crate) fn process_use_decl(&mut self, use_decl: &UseDecl) -> ElabResult<()> {
        // Determine which module this use statement belongs to
        // Look it up by the use statement's span (start, end) for uniqueness
        let use_module = self
            .env
            .get_use_statement_module(use_decl.span.start, use_decl.span.end)
            .cloned()
            .unwrap_or_else(ModulePath::root);

        // Set current_module for error reporting
        self.current_module = use_module.clone();

        // Determine if this is a re-export (`pub use` vs private `use`)
        let is_reexport = matches!(use_decl.visibility, Visibility::Public | Visibility::Crate);

        // Expand the use tree into paths or glob imports
        let expanded = use_decl.tree.expand();
        match expanded {
            ExpandedUseTree::Paths(paths) => {
                for path in paths {
                    self.process_use_path(&path, &use_module, is_reexport)?;
                }
            }
            ExpandedUseTree::Glob { prefix, span } => {
                self.process_use_glob(&prefix, span, &use_module, is_reexport)?;
            }
        }

        Ok(())
    }

    /// Process a glob import (`use foo::*;`), importing all public items from a module.
    ///
    /// This imports:
    /// - All types, values, and constructors defined in the module with pub visibility
    /// - All types, values, and constructors defined with pub(crate) visibility (within same crate)
    /// - All public re-exports from the module (items imported with `pub use`)
    ///
    /// The `is_reexport` parameter indicates whether the glob import itself is public (`pub use foo::*;`).
    /// This determines whether items imported via this glob will be re-exported to other modules.
    fn process_use_glob(
        &mut self,
        prefix: &ast::Path,
        span: crate::span::Span,
        use_module: &ModulePath,
        is_reexport: bool,
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

        // Import all types from the module
        // Use visibility stored in ModuleContents from module info building
        for type_name in &contents.types {
            // Get visibility from ModuleContents
            // If not found (shouldn't happen), assume public for workspace modules
            let visibility = contents
                .type_visibility
                .get(type_name)
                .cloned()
                .unwrap_or(Visibility::Public);

            if self.is_glob_importable(visibility, &source_module, use_module) {
                self.try_glob_import_type(
                    type_name.clone(),
                    &source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        // Import all values from the module
        for value_name in &contents.values {
            // Get visibility from ModuleContents
            let visibility = contents
                .value_visibility
                .get(value_name)
                .cloned()
                .unwrap_or(Visibility::Public);

            if self.is_glob_importable(visibility, &source_module, use_module) {
                self.try_glob_import_value(
                    value_name.clone(),
                    &source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        // Import all constructors
        // Constructors inherit visibility from their parent type
        for ctor_name in &contents.constructors {
            // Get visibility from ModuleContents (set during module info building)
            let visibility = contents
                .constructor_visibility
                .get(ctor_name)
                .cloned()
                .unwrap_or(Visibility::Public);

            if self.is_glob_importable(visibility, &source_module, use_module) {
                self.try_glob_import_constructor(
                    ctor_name.clone(),
                    &source_module,
                    use_module,
                    span,
                    is_reexport,
                )?;
            }
        }

        // Import public re-exports (types)
        // Only import re-exports that were marked as public (`pub use`)
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

        // Import public re-exports (values)
        // Only import re-exports that were marked as public (`pub use`)
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

        // Import public re-exports (constructors)
        // Only import re-exports that were marked as public (`pub use`)
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
            name.clone(),
            source_module.clone(),
            name,
            span,
            is_reexport,
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
            name.clone(),
            source_module.clone(),
            name,
            span,
            is_reexport,
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
            name.clone(),
            source_module.clone(),
            name,
            span,
            is_reexport,
        );
        Ok(())
    }

    /// Process a single use path, importing the item it refers to.
    ///
    /// `is_reexport` indicates whether this is a `pub use` (re-export) or private `use`.
    fn process_use_path(
        &mut self,
        path: &ast::Path,
        use_module: &ModulePath,
        is_reexport: bool,
    ) -> ElabResult<()> {
        // A use path must have at least 2 segments (module::item)
        if path.segments.len() < 2 {
            // Single-segment use: `use foo;` - this would import from the current module
            // For now, treat single-segment use as a no-op (item is already in scope)
            return Ok(());
        }

        // Extract item name (last segment)
        let item_name = path.item_name().name.clone();

        // Get the module segments (everything except the item name)
        let module_segments: Vec<String> = path.segments[..path.segments.len() - 1]
            .iter()
            .map(|s| s.name.clone())
            .collect();

        // Resolve the module path relative to the use_module
        let module_path = self.resolve_use_module_path(&module_segments, use_module, path.span)?;

        // Check module visibility
        if !self
            .env
            .is_module_accessible(&module_path, use_module, true)
        {
            return Err(ElabError::private_module(
                path.span,
                module_path.to_string(),
                use_module.to_string(),
            ));
        }

        // Try to import as each kind of item
        // An item could be:
        // 1. A type (e.g., `use foo::MyType;`)
        // 2. A value/function (e.g., `use foo::my_fn;`)
        // 3. A constructor (e.g., `use foo::Some;`)

        let mut found = false;

        // Use the item name's span for precise error location
        let item_span = path.item_name().span;

        // Try to import as a type
        if self.env.has_type_in_module(&module_path, &item_name) {
            // Check for duplicate - now with full import info
            if let Some(existing) = self.env.lookup_type_import(use_module, &item_name) {
                return Err(ElabError::duplicate_import(
                    item_span,
                    item_name.clone(),
                    existing.import_span,
                    existing.source_module.to_string(),
                    module_path.to_string(),
                ));
            }
            // Add type import with span
            self.env.add_type_import(
                use_module,
                item_name.clone(),
                module_path.clone(),
                item_name.clone(),
                item_span,
                is_reexport,
            );
            found = true;
        }

        // Try to import as a value
        if self.env.has_value_in_module(&module_path, &item_name) {
            // Check for duplicate
            if let Some(existing) = self.env.lookup_value_import(use_module, &item_name) {
                return Err(ElabError::duplicate_import(
                    item_span,
                    item_name.clone(),
                    existing.import_span,
                    existing.source_module.to_string(),
                    module_path.to_string(),
                ));
            }
            // Add value import with span
            self.env.add_value_import(
                use_module,
                item_name.clone(),
                module_path.clone(),
                item_name.clone(),
                item_span,
                is_reexport,
            );
            found = true;
        }

        // Try to import as a constructor
        if self.env.has_constructor_in_module(&module_path, &item_name) {
            // Check for duplicate
            if let Some(existing) = self.env.lookup_constructor_import(use_module, &item_name) {
                return Err(ElabError::duplicate_import(
                    item_span,
                    item_name.clone(),
                    existing.import_span,
                    existing.source_module.to_string(),
                    module_path.to_string(),
                ));
            }
            // Add constructor import with span
            self.env.add_constructor_import(
                use_module,
                item_name.clone(),
                module_path.clone(),
                item_name.clone(),
                item_span,
                is_reexport,
            );
            found = true;
        }

        if !found {
            return Err(ElabError::item_not_in_module(
                path.span,
                module_path.to_string(),
                item_name,
            ));
        }

        Ok(())
    }

    /// Resolve a module path from a `use` statement relative to the importing module.
    ///
    /// Resolution order:
    /// 1. Handle `super::` prefix (navigate up from current module)
    /// 2. Try sibling resolution (relative to use_module's parent)
    /// 3. Fall back to absolute path (from crate root)
    /// 4. If both sibling and absolute exist, use sibling but warn
    ///
    /// Returns an error with a suggestion if the module is not found.
    fn resolve_use_module_path(
        &self,
        segments: &[String],
        use_module: &ModulePath,
        span: crate::span::Span,
    ) -> ElabResult<ModulePath> {
        use crate::utils::find_best_suggestion;

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
        let sibling_path = if let Some(parent) = use_module.parent() {
            Some(parent.join(&raw_path))
        } else {
            None
        };
        let sibling_exists = sibling_path
            .as_ref()
            .map(|p| self.env.has_module(p))
            .unwrap_or(false);

        // Try absolute resolution (from crate root)
        let absolute_exists = self.env.has_module(&raw_path);

        // Resolution priority: child > sibling > absolute
        match (child_exists, sibling_exists, absolute_exists) {
            (true, _, _) => {
                // Child module found - most specific match
                Ok(child_path)
            }
            (false, true, _) => {
                // Sibling module found
                Ok(sibling_path.unwrap())
            }
            (false, false, true) => {
                // Absolute module found
                Ok(raw_path)
            }
            (false, false, false) => {
                // None exist - produce error with suggestion
                let tried_paths = [
                    Some(child_path.to_string()),
                    sibling_path.as_ref().map(|p| p.to_string()),
                    Some(raw_path.to_string()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" or ");

                // Find a suggestion from all known modules
                let all_modules: Vec<String> =
                    self.env.all_module_paths().map(|p| p.to_string()).collect();

                let suggestion = find_best_suggestion(
                    &raw_path.to_string(),
                    all_modules.iter().map(|s| s.as_str()),
                );

                Err(ElabError::module_not_found_with_suggestion(
                    span,
                    tried_paths,
                    suggestion.map(|s| s.to_string()),
                ))
            }
        }
    }
}
