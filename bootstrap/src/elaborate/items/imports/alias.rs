//! Import alias handling: `use foo::Bar as Baz;`
//!
//! Split from mod.rs to keep file size under threshold (ADR 16.5.26b).

use crate::ast::{self, Visibility};
use crate::elaborate::env::{ImportRequest, ModulePath};
use crate::elaborate::error::{DuplicateImportInfo, ElabError};
use crate::elaborate::{ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Process an aliased use path: `use foo::Bar as Baz;`
    ///
    /// Resolves `path` in the source module but registers under `alias_name`.
    pub(crate) fn process_use_alias(
        &mut self,
        path: &ast::Path,
        alias_name: &str,
        use_module: &ModulePath,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<()> {
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
        let found_type = self.try_import_type_as(
            use_module,
            &item_name,
            alias_name,
            &module_path,
            item_span,
            is_reexport,
            reexport_vis,
        )?;
        let found_value = self.try_import_value_as(
            use_module,
            &item_name,
            alias_name,
            &module_path,
            item_span,
            is_reexport,
            reexport_vis,
        )?;
        let found_ctor = self.try_import_constructor_as(
            use_module,
            &item_name,
            alias_name,
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

    /// Import a type under an alias name. Looks up `original_name` but registers as `alias_name`.
    fn try_import_type_as(
        &mut self,
        use_module: &ModulePath,
        original_name: &str,
        alias_name: &str,
        module_path: &ModulePath,
        item_span: crate::span::Span,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<bool> {
        if !self.env.has_type_in_module(module_path, original_name) {
            return Ok(false);
        }
        if let Some(existing) = self.env.lookup_type_import(use_module, alias_name) {
            return Err(ElabError::duplicate_import(
                item_span,
                DuplicateImportInfo {
                    name: alias_name.to_string(),
                    source_name: original_name.to_string(),
                    first_import_span: existing.import_span,
                    first_source_module: existing.source_module.to_string(),
                    second_source_module: module_path.to_string(),
                },
            ));
        }
        self.env.add_type_import(
            use_module,
            ImportRequest {
                local_name: alias_name.to_string(),
                source_module: module_path.clone(),
                original_name: original_name.to_string(),
                span: item_span,
                is_reexport,
                reexport_visibility: reexport_vis,
            },
        );
        Ok(true)
    }

    /// Import a value under an alias name. Looks up `original_name` but registers as `alias_name`.
    fn try_import_value_as(
        &mut self,
        use_module: &ModulePath,
        original_name: &str,
        alias_name: &str,
        module_path: &ModulePath,
        item_span: crate::span::Span,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<bool> {
        if !self.env.has_value_in_module(module_path, original_name) {
            return Ok(false);
        }
        if let Some(existing) = self.env.lookup_value_import(use_module, alias_name) {
            return Err(ElabError::duplicate_import(
                item_span,
                DuplicateImportInfo {
                    name: alias_name.to_string(),
                    source_name: original_name.to_string(),
                    first_import_span: existing.import_span,
                    first_source_module: existing.source_module.to_string(),
                    second_source_module: module_path.to_string(),
                },
            ));
        }
        self.env.add_value_import(
            use_module,
            ImportRequest {
                local_name: alias_name.to_string(),
                source_module: module_path.clone(),
                original_name: original_name.to_string(),
                span: item_span,
                is_reexport,
                reexport_visibility: reexport_vis,
            },
        );
        Ok(true)
    }

    /// Import a constructor under an alias name. Looks up `original_name` but registers as `alias_name`.
    fn try_import_constructor_as(
        &mut self,
        use_module: &ModulePath,
        original_name: &str,
        alias_name: &str,
        module_path: &ModulePath,
        item_span: crate::span::Span,
        is_reexport: bool,
        reexport_vis: Option<Visibility>,
    ) -> ElabResult<bool> {
        if !self
            .env
            .has_constructor_in_module(module_path, original_name)
        {
            return Ok(false);
        }
        if let Some(existing) = self.env.lookup_constructor_import(use_module, alias_name) {
            return Err(ElabError::duplicate_import(
                item_span,
                DuplicateImportInfo {
                    name: alias_name.to_string(),
                    source_name: original_name.to_string(),
                    first_import_span: existing.import_span,
                    first_source_module: existing.source_module.to_string(),
                    second_source_module: module_path.to_string(),
                },
            ));
        }
        self.env.add_constructor_import(
            use_module,
            ImportRequest {
                local_name: alias_name.to_string(),
                source_module: module_path.clone(),
                original_name: original_name.to_string(),
                span: item_span,
                is_reexport,
                reexport_visibility: reexport_vis,
            },
        );
        Ok(true)
    }
}
