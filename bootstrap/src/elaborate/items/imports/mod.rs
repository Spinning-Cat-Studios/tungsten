//! Use Declaration Processing
//!
//! Handles processing of `use` declarations, resolving module paths
//! and adding imports to the environment.
//!
//! Glob import logic (`use foo::*;`) is in `imports_glob.rs`.

use crate::ast::{self, ExpandedUseTree, UseDecl, Visibility};

use crate::elaborate::env::{ImportRequest, ModulePath};
use crate::elaborate::error::{DuplicateImportInfo, ElabError};
use crate::elaborate::{ElabResult, Elaborator};

/// Emit a warning when child/sibling fallback resolves a relative import (ADR 10.5.26m §2.2).
/// Guard: only warns when raw != resolved, suppressing false positives for absolute imports.
pub(super) fn emit_relative_fallback_warning(
    raw: &ModulePath,
    resolved: &ModulePath,
    use_module: &ModulePath,
    strategy: &str,
) {
    if *resolved != *raw {
        eprintln!(
            "warning: relative import `{raw}` resolved via {strategy} as `{resolved}` from `{use_module}`"
        );
        eprintln!("  = help: use absolute import: `{resolved}`");
    }
}

/// Emit a P3b warning when all three lookups fail from a nested module (ADR 10.5.26m §2.2).
pub(super) fn emit_relative_fallback_failure(raw: &ModulePath, use_module: &ModulePath) {
    if use_module.segments.len() >= 2 {
        eprintln!("warning: relative import `{raw}` could not be resolved from `{use_module}`");
        eprintln!(
            "  = help: use absolute import rooted at a top-level module \
(e.g., `parser::`, `elab::`, `driver::`, `lexer::`)"
        );
    }
}

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
        let reexport_vis = if is_reexport {
            Some(use_decl.visibility)
        } else {
            None
        };

        // Expand the use tree into paths or glob imports
        let expanded_items = use_decl.tree.expand_all();
        for expanded in expanded_items {
            match expanded {
                ExpandedUseTree::Paths(paths) => {
                    for path in paths {
                        self.process_use_path(&path, &use_module, is_reexport, reexport_vis)?;
                    }
                }
                ExpandedUseTree::Glob { prefix, span } => {
                    self.process_use_glob(&prefix, span, &use_module, is_reexport, reexport_vis)?;
                }
                ExpandedUseTree::Alias { path, alias, .. } => {
                    self.process_use_alias(
                        &path,
                        &alias.name,
                        &use_module,
                        is_reexport,
                        reexport_vis,
                    )?;
                }
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
        let reexport_vis = if is_reexport {
            Some(use_decl.visibility)
        } else {
            None
        };

        // Expand the use tree into paths or glob imports
        let expanded_items = use_decl.tree.expand_all();
        for expanded in expanded_items {
            match expanded {
                ExpandedUseTree::Paths(paths) => {
                    for path in paths {
                        self.process_use_path(&path, &use_module, is_reexport, reexport_vis)?;
                    }
                }
                ExpandedUseTree::Glob { prefix, span } => {
                    self.process_use_glob(&prefix, span, &use_module, is_reexport, reexport_vis)?;
                }
                ExpandedUseTree::Alias { path, alias, .. } => {
                    self.process_use_alias(
                        &path,
                        &alias.name,
                        &use_module,
                        is_reexport,
                        reexport_vis,
                    )?;
                }
            }
        }

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
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<()> {
        // A use path must have at least 2 segments (module::item)
        if path.segments.len() < 2 {
            return Ok(());
        }

        let item_name = path.item_name().name.clone();
        let module_segments: Vec<String> = path.segments[..path.segments.len() - 1]
            .iter()
            .map(|s| s.name.clone())
            .collect();
        let module_path = self.resolve_use_module_path(&module_segments, use_module, path.span)?;

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

        let item_span = path.item_name().span;
        let found_type = self.try_import_type(
            use_module,
            &item_name,
            &module_path,
            item_span,
            is_reexport,
            reexport_vis,
        )?;
        let found_value = self.try_import_value(
            use_module,
            &item_name,
            &module_path,
            item_span,
            is_reexport,
            reexport_vis,
        )?;
        let found_ctor = self.try_import_constructor(
            use_module,
            &item_name,
            &module_path,
            item_span,
            is_reexport,
            reexport_vis,
        )?;

        if !found_type && !found_value && !found_ctor {
            return Err(ElabError::item_not_in_module(
                path.span,
                module_path.to_string(),
                item_name,
            ));
        }

        Ok(())
    }

    fn try_import_type(
        &mut self,
        use_module: &ModulePath,
        item_name: &str,
        module_path: &ModulePath,
        item_span: crate::span::Span,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<bool> {
        if !self.env.has_type_in_module(module_path, item_name) {
            return Ok(false);
        }
        if let Some(existing) = self.env.lookup_type_import(use_module, item_name) {
            return Err(ElabError::duplicate_import(
                item_span,
                DuplicateImportInfo {
                    name: item_name.to_string(),
                    source_name: item_name.to_string(),
                    first_import_span: existing.import_span,
                    first_source_module: existing.source_module.to_string(),
                    second_source_module: module_path.to_string(),
                },
            ));
        }
        self.env.add_type_import(
            use_module,
            ImportRequest {
                local_name: item_name.to_string(),
                source_module: module_path.clone(),
                original_name: item_name.to_string(),
                span: item_span,
                is_reexport,
                reexport_visibility: reexport_vis,
            },
        );
        Ok(true)
    }

    fn try_import_value(
        &mut self,
        use_module: &ModulePath,
        item_name: &str,
        module_path: &ModulePath,
        item_span: crate::span::Span,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<bool> {
        if !self.env.has_value_in_module(module_path, item_name) {
            return Ok(false);
        }
        if let Some(existing) = self.env.lookup_value_import(use_module, item_name) {
            return Err(ElabError::duplicate_import(
                item_span,
                DuplicateImportInfo {
                    name: item_name.to_string(),
                    source_name: item_name.to_string(),
                    first_import_span: existing.import_span,
                    first_source_module: existing.source_module.to_string(),
                    second_source_module: module_path.to_string(),
                },
            ));
        }
        self.env.add_value_import(
            use_module,
            ImportRequest {
                local_name: item_name.to_string(),
                source_module: module_path.clone(),
                original_name: item_name.to_string(),
                span: item_span,
                is_reexport,
                reexport_visibility: reexport_vis,
            },
        );
        Ok(true)
    }

    fn try_import_constructor(
        &mut self,
        use_module: &ModulePath,
        item_name: &str,
        module_path: &ModulePath,
        item_span: crate::span::Span,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<bool> {
        if !self.env.has_constructor_in_module(module_path, item_name) {
            return Ok(false);
        }
        if let Some(existing) = self.env.lookup_constructor_import(use_module, item_name) {
            return Err(ElabError::duplicate_import(
                item_span,
                DuplicateImportInfo {
                    name: item_name.to_string(),
                    source_name: item_name.to_string(),
                    first_import_span: existing.import_span,
                    first_source_module: existing.source_module.to_string(),
                    second_source_module: module_path.to_string(),
                },
            ));
        }
        self.env.add_constructor_import(
            use_module,
            ImportRequest {
                local_name: item_name.to_string(),
                source_module: module_path.clone(),
                original_name: item_name.to_string(),
                span: item_span,
                is_reexport,
                reexport_visibility: reexport_vis,
            },
        );
        Ok(true)
    }
}

mod alias;
mod resolve;
