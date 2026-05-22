//! Module-system error constructors.
//!
//! Convenience constructors for module, import, and visibility errors.

use crate::span::Span;

use super::{ElabError, ElabErrorKind};

/// Bundled information for a duplicate-import diagnostic.
pub struct DuplicateImportInfo {
    pub name: String,
    pub source_name: String,
    pub first_import_span: Span,
    pub first_source_module: String,
    pub second_source_module: String,
}

impl ElabError {
    /// Create a "module not found" error.
    pub fn module_not_found(span: Span, module: impl Into<String>) -> Self {
        Self::new(
            span,
            ElabErrorKind::ModuleNotFound {
                module: module.into(),
                suggestion: None,
            },
        )
    }

    /// Create a "module not found" error with a suggestion.
    pub fn module_not_found_with_suggestion(
        span: Span,
        module: impl Into<String>,
        suggestion: Option<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::ModuleNotFound {
                module: module.into(),
                suggestion,
            },
        )
    }

    /// Create an "item not found in module" error.
    pub fn item_not_in_module(
        span: Span,
        module: impl Into<String>,
        item: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::ItemNotFoundInModule {
                module: module.into(),
                item: item.into(),
            },
        )
    }

    /// Create a "duplicate import" error showing both import locations.
    ///
    /// The error will show:
    /// - Primary span at `second_import_span` (the import that triggered the error)
    /// - Secondary note at `first_import_span` showing where it was first imported
    /// - Different message format when imports come from different modules
    ///
    /// `name` is the local alias that clashed; `source_name` is the actual item
    /// name in the second module (may differ when `as` aliasing is used).
    pub fn duplicate_import(second_import_span: Span, info: DuplicateImportInfo) -> Self {
        let mut err = Self::new(
            second_import_span,
            ElabErrorKind::DuplicateImport {
                name: info.name.clone(),
                first_import_span: info.first_import_span,
                second_import_span,
                first_source_module: info.first_source_module.clone(),
                second_source_module: info.second_source_module.clone(),
            },
        );

        // Add note showing first import location
        if info.first_source_module == info.second_source_module {
            err = err.with_span_note(info.first_import_span, "first imported here");
        } else {
            err = err.with_span_note(
                info.first_import_span,
                format!("first imported from `{}`", info.first_source_module),
            );
        }

        // Add help suggesting `as` rename — use source_name for the path
        // so `use b::Bar as Foo` suggests `use b::Bar as bar_alias`, not `use b::Foo ...`
        err = err.with_help(format!(
            "use `as` to rename one import: `use {}::{} as {}_alias;`",
            info.second_source_module,
            info.source_name,
            info.source_name.to_lowercase()
        ));

        err
    }

    /// Create a "glob conflict" error for when two glob imports bring in the same name.
    pub fn glob_conflict(
        span: Span,
        name: impl Into<String>,
        first_module: impl Into<String>,
        second_module: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::GlobConflict {
                name: name.into(),
                first_module: first_module.into(),
                second_module: second_module.into(),
            },
        )
    }

    /// Create an "unresolved import" error.
    pub fn unresolved_import(span: Span, path: impl Into<String>) -> Self {
        Self::new(span, ElabErrorKind::UnresolvedImport(path.into()))
    }

    /// Create a "private module" error.
    pub fn private_module(
        span: Span,
        module_path: impl Into<String>,
        accessed_from: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::PrivateModule {
                module_path: module_path.into(),
                accessed_from: accessed_from.into(),
            },
        )
    }

    /// Create a "private item" error.
    pub fn private_item(
        span: Span,
        item_name: impl Into<String>,
        item_kind: impl Into<String>,
        defined_in: impl Into<String>,
        accessed_from: impl Into<String>,
    ) -> Self {
        Self::new(
            span,
            ElabErrorKind::PrivateItem {
                item_name: item_name.into(),
                item_kind: item_kind.into(),
                defined_in: defined_in.into(),
                accessed_from: accessed_from.into(),
            },
        )
    }
}
