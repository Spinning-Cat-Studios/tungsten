//! Collection pass for the elaborator.
//!
//! Two-phase name registration: first register type names (Phase 1a),
//! then process imports, elaborate type bodies, and collect values.

use crate::ast::{Item, SourceFile};

use super::env;
use super::{CoreDef, ElabError, ElabResult, Elaborator};

impl<'a> Elaborator<'a> {
    /// Elaborate an entire source file.
    ///
    /// Two-pass algorithm:
    /// 1. Collect all top-level type and value definitions
    /// 2. Elaborate each definition, resolving names and inferring types
    pub fn elaborate_file(&mut self, file: &SourceFile) -> Result<Vec<CoreDef>, Vec<ElabError>> {
        // Pass 1: Collect all top-level definitions
        self.run_collection_pass(file)?;

        // Pass 2: Elaborate each definition
        let mut defs = Vec::new();
        for item in &file.items {
            match self.elaborate_item(item) {
                Ok(Some(def)) => defs.push(def),
                Ok(None) => {} // Type definitions don't produce CoreDefs
                Err(e) => self.record_error(e),
            }
        }

        if self.errors.is_empty() {
            Ok(defs)
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Run only the collection pass (first pass).
    ///
    /// This collects all type and value definitions into the environment.
    /// After collection, it validates that public item signatures don't leak
    /// private types (export validation).
    pub fn run_collection_pass(&mut self, file: &SourceFile) -> Result<(), Vec<ElabError>> {
        // Phase 1a: Register type NAMES only (without elaborating bodies)
        // This allows imports to reference types that exist but aren't fully defined yet
        for item in &file.items {
            if matches!(item, Item::TypeDef(_) | Item::TypeAlias(_)) {
                if let Err(e) = self.register_type_name(item) {
                    self.record_error(e);
                }
            }
        }
        self.check_phase_1a(); // ADR 20.4.26e

        // Phase 1b: Process use declarations
        // Now that all type names are registered, we can resolve imports
        // Use enumerate to track item index for file provenance lookup
        for (index, item) in file.items.iter().enumerate() {
            if let Item::Use(use_decl) = item {
                if let Err(e) = self.process_use_decl_with_index(use_decl, index) {
                    self.record_error(e);
                }
            }
        }
        self.check_phase_1b(); // ADR 20.4.26e

        // Phase 1c: Fully collect TYPE definitions (elaborate bodies)
        // Now that imports are available, type bodies can reference imported types
        // During this phase, ADT cross-references are deferred as TyVar("@Name")
        // so mutual recursion groups can be computed before encoding (ADR 18.4.26i §5).
        self.collection_phase = true;
        for item in &file.items {
            if matches!(item, Item::TypeDef(_) | Item::TypeAlias(_)) {
                if let Err(e) = self.collect_item(item) {
                    self.record_error(e);
                }
            }
        }
        self.collection_phase = false;
        self.check_phase_1c(); // ADR 20.4.26e

        // Phase 1c.5: Compute mutual recursion groups (ADR 18.4.26i §5 Step 3)
        // After all types are elaborated, identify mutually recursive type clusters
        // (SCCs of size > 1 in the type dependency graph). This information is used
        // by encode_adt_type to produce nested μ-binder encodings.
        self.compute_mutual_recursion_groups();
        self.check_phase_1c5(); // ADR 20.4.26e

        // Phase 1d: Resolve deferred type references
        // During Phase 1c, some types may have been elaborated before the types they
        // reference (due to AST order). Those references were stored as TyVars.
        // Now that all types are fully elaborated, resolve those TyVars.
        self.resolve_deferred_type_references();
        self.check_phase_1d(); // ADR 20.4.26e

        // Phase 1e: Cache encoded types for non-parameterized types
        // This enables reverse lookup from Core types to user-defined names
        // for cleaner error messages.
        self.cache_type_encodings();
        self.check_phase_1e(); // ADR 20.4.26e

        // Constructor metadata integrity check (ADR 7.5.26f)
        self.check_constructor_metadata();

        // Phase 2: Collect all VALUE definitions (functions, theorems, etc.)
        // Now that types and imports are available, function signatures can reference them
        for item in &file.items {
            if !matches!(item, Item::TypeDef(_) | Item::TypeAlias(_) | Item::Use(_)) {
                if let Err(e) = self.collect_item(item) {
                    self.record_error(e);
                }
            }
        }

        // Validate export signatures (detect public items leaking private types)
        let export_errors = self.validate_export_signatures(&file.items);
        self.errors.extend(export_errors);

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// Register a type name without elaborating its body (Phase 1a).
    ///
    /// This makes the type name available for import resolution,
    /// but the actual type body is elaborated later in Phase 1c.
    fn register_type_name(&mut self, item: &Item) -> ElabResult<()> {
        // Set current_module based on the item's location for error reporting
        if let Some(name) = self.get_item_name(item) {
            if let Some(module_path) = self.env.get_item_module(&name) {
                self.current_module = module_path.clone();
            }
        }

        match item {
            Item::TypeDef(type_def) => {
                // Check for duplicates - but allow replacing stubs (from workspace sibling modules)
                // and Phase A placeholder ADTs (ADR 5.5.26c: encoded_type is None for placeholders)
                if let Some(existing) = self.env.lookup_type(&type_def.name.name) {
                    let is_overwritable = matches!(existing.kind, env::TypeDefKind::Stub)
                        || existing.encoded_type.is_none()
                        || existing.defining_module.is_none();
                    if !is_overwritable {
                        return Err(ElabError::duplicate(
                            type_def.name.span,
                            &type_def.name.name,
                        ));
                    }
                }
                // Extract type parameter names for arity tracking
                let params: Vec<String> = type_def
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect();
                // Register as a stub - will be replaced in Phase 1c
                self.env.register_type_stub(
                    &type_def.name.name,
                    params,
                    type_def.visibility,
                    type_def.span,
                );
                Ok(())
            }
            Item::TypeAlias(alias) => {
                // Check for duplicates - but allow replacing stubs (from workspace sibling modules)
                // and Phase A placeholder types (ADR 5.5.26c: encoded_type is None for placeholders)
                if let Some(existing) = self.env.lookup_type(&alias.name.name) {
                    let is_overwritable = matches!(existing.kind, env::TypeDefKind::Stub)
                        || existing.encoded_type.is_none()
                        || existing.defining_module.is_none();
                    if !is_overwritable {
                        return Err(ElabError::duplicate(alias.name.span, &alias.name.name));
                    }
                }
                // Extract type parameter names for arity tracking
                let params: Vec<String> = alias
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect();
                self.env
                    .register_type_stub(&alias.name.name, params, alias.visibility, alias.span);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Compute mutual recursion groups from the type dependency graph (ADR 18.4.26i §5 Step 3).
    ///
    /// Builds a type dependency graph from the elaborated ADT definitions,
    /// runs Tarjan's SCC algorithm, and stores groups of size > 1 in
    /// `self.mutual_recursion_groups`. Each type in such a group maps to
    /// the full sorted list of group members.
    fn compute_mutual_recursion_groups(&mut self) {
        use crate::doctor::audit_mutual_types::scc::tarjan_scc;
        use crate::doctor::audit_mutual_types::type_graph::TypeGraph;

        let adt_types = self.get_adt_types();
        let graph = TypeGraph::build(&adt_types);
        let sccs = tarjan_scc(&graph);

        for scc in &sccs {
            if scc.len() > 1 {
                // scc is already sorted (tarjan_scc sorts components)
                for member in scc {
                    self.mutual_recursion_groups
                        .insert(member.clone(), scc.clone());
                }
            }
        }
    }
}
