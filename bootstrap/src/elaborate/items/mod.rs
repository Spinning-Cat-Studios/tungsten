//! Item Elaboration: Functions, Theorems, Type Definitions
//!
//! Handles top-level item collection and elaboration in two passes:
//! - **Pass 1 (Collection):** Populate the environment with type/value signatures
//! - **Pass 2 (Elaboration):** Build Core terms from item bodies
//!
//! # Organization
//!
//! - `mod.rs` — Entry points and dispatch
//! - `collect.rs` — Pass 1 collection functions
//! - `elaborate.rs` — Pass 2 elaboration functions
//! - `type_building.rs` — Type signature construction helpers
//! - `extern_fn.rs` — FFI external function handling
//! - `imports.rs` — Use declaration processing
//! - `export_validation.rs` — Public item visibility leak detection

mod collect;
mod elaborate;
mod export_validation;
mod extern_fn;
mod imports;
mod imports_glob;
mod type_building;

use crate::ast::Item;

use super::{CoreDef, DefTerminationMeta, ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    // ─────────────────────────────────────────────────────────────────────────
    // Pass 1: Collect definitions
    // ─────────────────────────────────────────────────────────────────────────

    /// Collect a top-level item's signature (first pass).
    ///
    /// This populates the environment with type and value signatures
    /// so that items can refer to each other (forward references).
    pub(crate) fn collect_item(&mut self, item: &Item) -> ElabResult<()> {
        // Set current_module based on the item's location for error reporting
        if let Some(name) = self.get_item_name(item) {
            if let Some(module_path) = self.env.get_item_module(&name) {
                self.current_module = module_path.clone();
            }
        }

        match item {
            Item::Function(func) => self.collect_function(func),
            Item::TypeDef(type_def) => self.collect_type_def(type_def),
            Item::TypeAlias(alias) => self.collect_type_alias(alias),
            Item::Theorem(thm) => self.collect_theorem(thm),
            Item::Lemma(lemma) => self.collect_theorem(lemma), // Same as theorem
            Item::Axiom(axiom) => self.collect_axiom(axiom),
            Item::ExternFn(extern_fn) => self.collect_extern_fn(extern_fn),
            Item::Mod(_) => Ok(()), // Module declarations handled during parsing
            Item::Use(_) => Ok(()), // Use declarations processed by process_use_decls
            Item::Error(_) => Ok(()), // Skip error nodes
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Pass 2: Elaborate definitions
    // ─────────────────────────────────────────────────────────────────────────

    /// Elaborate a top-level item to a Core definition.
    ///
    /// Returns Some(CoreDef) for value items, None for type definitions.
    pub(crate) fn elaborate_item(&mut self, item: &Item) -> ElabResult<Option<CoreDef>> {
        self.setup_item_context(item);
        self.record_termination_meta(item);

        match item {
            Item::Function(func) => Ok(Some(self.elaborate_function(func)?)),
            Item::Theorem(thm) => Ok(Some(self.elaborate_theorem(thm)?)),
            Item::Lemma(lemma) => Ok(Some(self.elaborate_theorem(lemma)?)),
            Item::Axiom(axiom) => Ok(Some(self.elaborate_axiom(axiom)?)),
            Item::ExternFn(extern_fn) => Ok(Some(self.elaborate_extern_fn(extern_fn)?)),
            Item::TypeDef(_) | Item::TypeAlias(_) => Ok(None), // Types don't produce CoreDefs
            Item::Mod(_) => Ok(None), // Module declarations handled during parsing
            Item::Use(_) => Ok(None), // Use declarations: Phase 4 will process imports
            Item::Error(_) => Ok(None),
        }
    }

    /// Record an item's termination annotations and proof-relevance
    /// (ADR 29.6.26e).
    ///
    /// Only items with a **non-default** entry are stored: the gate treats an
    /// absent entry as "executable, unannotated", so recording every function
    /// would cost a map entry per definition to say nothing. Proof items are
    /// always recorded, because "is a proof" is not the default.
    fn record_termination_meta(&mut self, item: &Item) {
        let (name, meta) = match item {
            Item::Function(func) if !func.attrs.is_empty() => (
                func.name.name.clone(),
                DefTerminationMeta {
                    attrs: func.attrs.clone(),
                    is_proof: false,
                },
            ),
            Item::Theorem(def) | Item::Lemma(def) => (
                def.name.name.clone(),
                DefTerminationMeta {
                    attrs: crate::ast::TerminationAttrs::default(),
                    is_proof: true,
                },
            ),
            Item::Axiom(def) => (
                def.name.name.clone(),
                DefTerminationMeta {
                    attrs: crate::ast::TerminationAttrs::default(),
                    is_proof: true,
                },
            ),
            _ => return,
        };
        self.termination_meta.insert(name, meta);
    }

    /// Set up tracing and module context for the current item.
    fn setup_item_context(&mut self, item: &Item) {
        // Track current definition name for --trace-types (ADR 13.4.26c §5)
        self.current_def_name = self.get_item_name(item);

        // Set current_module based on the item's location
        // This is used for visibility checking in qualified path resolution
        if let Some(name) = self.get_item_name(item) {
            if let Some(module_path) = self.env.get_item_module(&name) {
                self.current_module = module_path.clone();
            }
        }
    }

    /// Get the name of an item (for looking up its module).
    pub fn get_item_name(&self, item: &Item) -> Option<String> {
        match item {
            Item::Function(f) => Some(f.name.name.clone()),
            Item::Theorem(t) | Item::Lemma(t) => Some(t.name.name.clone()),
            Item::Axiom(a) => Some(a.name.name.clone()),
            Item::ExternFn(e) => Some(e.name.name.clone()),
            Item::TypeDef(t) => Some(t.name.name.clone()),
            Item::TypeAlias(t) => Some(t.name.name.clone()),
            _ => None,
        }
    }
}
