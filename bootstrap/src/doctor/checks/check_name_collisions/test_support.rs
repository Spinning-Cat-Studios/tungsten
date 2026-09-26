//! Shared fixtures for the check's two test modules.
//!
//! [`TreeBuilder`] builds the two fields the census reads — `modules` and
//! `item_modules` — the way the driver's own walk does, `insert`-last-wins
//! included, so a test that asserts on the winner is asserting about the real
//! rule rather than about the builder.

use std::collections::HashSet;

use crate::ast::Visibility;
use crate::driver::modules::ModuleInfo;
use crate::elaborate::{ModuleContents, ModulePath};

use super::census::{census, Census, ReexportHandling};

/// A module path from `::`-separated segments.
pub(super) fn path(segments: &str) -> ModulePath {
    ModulePath::new(segments.split("::").map(str::to_string).collect())
}

/// Builder for the two fields the census reads.
#[derive(Default)]
pub(super) struct TreeBuilder {
    info: ModuleInfo,
    externs: HashSet<(ModulePath, String)>,
}

impl TreeBuilder {
    /// Define `name` in `module` with `visibility`, as the walk would.
    pub(super) fn define(mut self, module: &str, name: &str, visibility: Visibility) -> Self {
        let path = path(module);
        let contents: &mut ModuleContents = self.info.modules.entry(path.clone()).or_default();
        contents.values.push(name.to_string());
        contents
            .value_visibility
            .insert(name.to_string(), visibility);
        // Last writer wins, exactly as `register_value_item` does.
        self.info.item_modules.insert(name.to_string(), path);
        self
    }

    /// The same, but as an `extern "C" fn`.
    pub(super) fn define_extern(
        mut self,
        module: &str,
        name: &str,
        visibility: Visibility,
    ) -> Self {
        self.externs.insert((path(module), name.to_string()));
        self.define(module, name, visibility)
    }

    /// Record `name` in `module` as a `pub use`-synthesized copy of a
    /// definition in `source`, as `copy_contents_entries` does.
    pub(super) fn reexport(mut self, module: &str, name: &str, source: &str) -> Self {
        let path = path(module);
        let contents: &mut ModuleContents = self.info.modules.entry(path).or_default();
        contents.values.push(name.to_string());
        contents
            .value_visibility
            .insert(name.to_string(), Visibility::Public);
        contents
            .reexported_value_sources
            .insert(name.to_string(), (self::path(source), name.to_string()));
        self
    }

    /// Register a module that defines nothing, so `modules_examined` counts it.
    pub(super) fn empty_module(mut self, module: &str) -> Self {
        self.info.modules.entry(path(module)).or_default();
        self
    }
}

impl TreeBuilder {
    /// Census the built tree. A method rather than public fields, so a test
    /// reads as a question about the tree and not about the builder.
    pub(super) fn census(&self, reexports: ReexportHandling) -> Census {
        census(&self.info, &self.externs, reexports)
    }

    /// Drop `name` from the flat `item_modules` table, leaving the definitions
    /// in place. The real walk always records one, so this is the only way to
    /// reach the no-recorded-winner arm.
    pub(super) fn forget_flat_entry(&mut self, name: &str) {
        self.info.item_modules.remove(name);
    }

    /// Point the flat `item_modules` table at `module` for `name`, whatever the
    /// value definitions say. The flat table is shared with types, so a
    /// same-named type registered later really can take the slot.
    pub(super) fn shadow_flat_entry(mut self, name: &str, module: &str) -> Self {
        self.info
            .item_modules
            .insert(name.to_string(), path(module));
        self
    }
}
