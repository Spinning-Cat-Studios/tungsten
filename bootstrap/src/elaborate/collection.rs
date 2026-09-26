//! Collection pass for the elaborator.
//!
//! Two-phase name registration: first register type names (Type-Name Registration),
//! then process imports, elaborate type bodies, and collect values.

use crate::ast::{Item, SourceFile};

use super::env;
use super::error::ElabErrorKind;
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
        let defs = self.run_body_pass(&file.items);

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
        // Type-Name Registration: Register type NAMES only (without elaborating bodies)
        // This allows imports to reference types that exist but aren't fully defined yet
        for item in &file.items {
            if matches!(item, Item::TypeDef(_) | Item::TypeAlias(_)) {
                if let Err(e) = self.register_type_name(item) {
                    self.record_error(e);
                }
            }
        }
        self.check_type_name_registration(); // ADR 20.4.26e

        // Import Resolution: Process use declarations
        // Now that all type names are registered, we can resolve imports
        // Use enumerate to track item index for file provenance lookup
        for (index, item) in file.items.iter().enumerate() {
            if let Item::Use(use_decl) = item {
                if let Err(e) = self.process_use_decl_with_index(use_decl, index) {
                    self.record_error(e);
                }
            }
        }
        self.check_import_resolution(); // ADR 20.4.26e

        // Type-Body Collection: Fully collect TYPE definitions (elaborate bodies)
        // Now that imports are available, type bodies can reference imported types
        // During this phase, ADT cross-references are deferred as TyVar("@Name")
        // so mutual recursion groups can be computed before encoding (ADR 18.4.26i §5).
        self.collecting_type_bodies = true;
        for item in &file.items {
            if matches!(item, Item::TypeDef(_) | Item::TypeAlias(_)) {
                if let Err(e) = self.collect_item(item) {
                    self.record_error(e);
                    self.poison_failed_type_body(item);
                }
            }
        }
        self.collecting_type_bodies = false;
        self.check_type_body_collection(); // ADR 20.4.26e

        // Recursion Grouping: Compute mutual recursion groups (ADR 18.4.26i §5 Step 3)
        // After all types are elaborated, identify mutually recursive type clusters
        // (SCCs of size > 1 in the type dependency graph). This information is used
        // by encode_adt_type to produce nested μ-binder encodings.
        self.compute_mutual_recursion_groups();
        self.check_recursion_grouping(); // ADR 20.4.26e

        // Strict Positivity: reject type definitions whose own SCC is reachable
        // from a forbidden position (ADR 7.8.26e). This is the only slot that
        // works: earlier and the SCCs do not exist, so the mutual case is
        // invisible; later than Deferred-TyVar Resolution and the @-prefixed
        // cross-references have already been rewritten.
        self.check_strict_positivity();

        // Deferred-TyVar Resolution: Resolve deferred type references
        // During Type-Body Collection, some types may have been elaborated before the types they
        // reference (due to AST order). Those references were stored as TyVars.
        // Now that all types are fully elaborated, resolve those TyVars.
        self.resolve_deferred_type_references();
        self.check_deferred_tyvar_resolution(); // ADR 20.4.26e

        // Encoding Finalization: Cache encoded types for non-parameterized types
        // This enables reverse lookup from Core types to user-defined names
        // for cleaner error messages.
        self.cache_type_encodings();
        self.check_encoding_finalization(); // ADR 20.4.26e

        // Constructor metadata integrity check (ADR 7.5.26f)
        self.check_constructor_metadata();

        // Phase 2: Collect all VALUE definitions (functions, theorems, etc.)
        // Now that types and imports are available, function signatures can reference them
        for item in &file.items {
            if !matches!(item, Item::TypeDef(_) | Item::TypeAlias(_) | Item::Use(_)) {
                if let Err(e) = self.collect_item(item) {
                    self.record_error(e);
                    self.poison_failed_signature(item);
                }
            }
        }

        // Validate export signatures (detect public items leaking private types)
        let export_errors = self.validate_export_signatures(&file.items);
        self.errors.extend(export_errors);

        self.collection_pass_verdict()
    }

    /// The collection pass's exit: defer or short-circuit (ADR 14.8.26g D2).
    ///
    /// The short-circuit is deferred when — and only when — every recorded
    /// error was compensated by a poison registration (the conditional form;
    /// D3's producers are what compensate). Deferred errors stay in
    /// `self.errors` — NOT drained — and Pass 2 runs against the poisoned
    /// environment, so a dependent module the accumulating walk (D1) reaches
    /// compares against `Type::Error` and stays quiet instead of cascading.
    /// The errors flow out exactly once, when the entry point drains them
    /// (`elaborate_file` / `elaborate` / `elaborate_with_exports`); callers
    /// that never run Pass 2 must consult `self.errors` explicitly (the D2a
    /// audit — see `CollectedElaborator::has_collection_errors`).
    ///
    /// An UNPOISONED error still short-circuits: it means a name is simply
    /// missing (a duplicate, a failed import, a registration error), and
    /// running Pass 2 against that would cascade rather than suppress.
    ///
    /// The GLOBAL Signature Collection pass opts out of the short-circuit
    /// (`defer_all_collection_errors`): it never runs Pass 2, so deferring
    /// costs nothing there — while short-circuiting throws away every
    /// signature it DID collect, and the accumulating walk then amplifies
    /// that missing environment across every module. Measured: V4's one
    /// unresolved `use` went 1 → 894 reported errors when the global pass
    /// short-circuited; deferring restores the partial exports and the 1.
    fn collection_pass_verdict(&mut self) -> Result<(), Vec<ElabError>> {
        if self.errors.is_empty()
            || self.defer_all_collection_errors
            || self.poison_compensated_error_count == self.errors.len()
        {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// D3's type-body poison producer (ADR 14.8.26g §2.2): a type whose
    /// Type-Body Collection failed keeps its Type-Name Registration stub, with
    /// `encoded_type = Some(Type::Error)` so downstream references compare
    /// against poison — which `types_equal`'s poison arms suppress — instead
    /// of cascading. The ADT's constructors are registered too (ADR 15.8.26d
    /// D2): a name that resolves reaches the constructor boundary's poison
    /// arm, while an unregistered one raised an unknown-value error at every
    /// call site — the parent's carried V5 residue.
    /// Deliberately does NOT fire when the existing definition is a real,
    /// finalized one (a duplicate definition failed, not this type's body):
    /// the first definition stands, and poisoning it would silence every use
    /// of a perfectly good type. Same rule the duplicate check itself uses —
    /// only an overwritable stub can be the one whose body just failed.
    fn poison_failed_type_body(&mut self, item: &Item) {
        let Some(name) = self.get_item_name(item) else {
            return;
        };
        let Some(def) = self.env.types.get_mut(&name) else {
            return;
        };
        if !def.is_overwritable_by_collection() {
            return;
        }
        def.encoded_type = Some(tungsten_core::Type::Error);
        self.register_poisoned_constructors(item);
        self.poison_compensated_error_count += 1;
    }

    /// Register the constructor NAMES of a poisoned ADT, so a call site
    /// resolves to the poisoned parent instead of an undefined value. An
    /// entry already present (Stub Registration's placeholder, in the
    /// per-module driver) is kept — it carries the same name and arity.
    fn register_poisoned_constructors(&mut self, item: &Item) {
        use crate::ast::TypeBody;
        use crate::elaborate::env::ConstructorInfo;
        let Item::TypeDef(type_def) = item else {
            return;
        };
        let TypeBody::Sum(variants) = &type_def.body else {
            return;
        };
        for (index, variant) in variants.iter().enumerate() {
            self.env
                .constructors
                .entry(variant.name.name.clone())
                .or_insert_with(|| ConstructorInfo {
                    type_name: type_def.name.name.clone(),
                    index,
                    arity: variant.fields.len(),
                    visibility: variant.visibility,
                    defining_module: None,
                });
        }
    }

    /// D3's signature poison producer (ADR 14.8.26g §2.2): a value whose
    /// signature failed to collect is registered as `Type::Error` rather than
    /// omitted, so uses of the name elaborate against poison instead of
    /// raising a cascade of undefined-value errors at every call site.
    ///
    /// Deliberately does NOT fire when the name is already defined (a
    /// duplicate definition): the first definition stands, nothing is
    /// missing, and overwriting a real signature with poison would silence
    /// every caller of a perfectly good function. The error stays unpoisoned,
    /// so the pass short-circuits as it did before the deferral.
    fn poison_failed_signature(&mut self, item: &Item) {
        use crate::elaborate::env::ValueDef;
        let Some(name) = self.get_item_name(item) else {
            return;
        };
        if self.env.values.contains_key(&name) {
            return;
        }
        let (visibility, span) = match item {
            Item::Function(f) => (f.visibility, f.name.span),
            Item::Theorem(t) | Item::Lemma(t) => (t.visibility, t.name.span),
            Item::Axiom(a) => (a.visibility, a.name.span),
            Item::ExternFn(e) => (e.visibility, e.name.span),
            _ => return,
        };
        self.env.values.insert(
            name.clone(),
            ValueDef {
                name,
                ty: tungsten_core::Type::Error,
                visibility,
                span,
            },
        );
        self.poison_compensated_error_count += 1;
    }

    /// Register a type name without elaborating its body (Type-Name Registration).
    ///
    /// This makes the type name available for import resolution,
    /// but the actual type body is elaborated later in Type-Body Collection.
    fn register_type_name(&mut self, item: &Item) -> ElabResult<()> {
        // Set current_module based on the item's location for error reporting
        if let Some(name) = self.get_item_name(item) {
            if let Some(module_path) = self.env.get_item_module(&name) {
                self.current_module = module_path.clone();
            }
        }

        // A `type` under a builtin's name would be unreachable, not a shadow:
        // builtin lookup runs first (ADR 14.9.26c). `Int` is the likely
        // collision — the FFI module's C-`int` alias was renamed `CInt` for it.
        let declared = match item {
            Item::TypeDef(type_def) => Some(&type_def.name),
            Item::TypeAlias(alias) => Some(&alias.name),
            _ => None,
        };
        if let Some(name) = declared {
            if Self::builtin_type(&name.name).is_some() {
                return Err(ElabError::new(
                    name.span,
                    ElabErrorKind::BuiltinTypeRedefined(name.name.clone()),
                ));
            }
        }

        match item {
            Item::TypeDef(type_def) => {
                // Check for duplicates - but allow replacing stubs (from workspace sibling modules)
                // and Stub Registration placeholder ADTs (ADR 5.5.26c: encoded_type is None for placeholders)
                if let Some(existing) = self.env.lookup_type(&type_def.name.name) {
                    if !existing.is_overwritable_by_collection() {
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
                // Register as a stub - will be replaced in Type-Body Collection
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
                // and Stub Registration placeholder types (ADR 5.5.26c: encoded_type is None for placeholders)
                if let Some(existing) = self.env.lookup_type(&alias.name.name) {
                    if !existing.is_overwritable_by_collection() {
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

    /// Whether every error the collection pass recorded was compensated by a
    /// poison producer (ADR 14.8.26g D2/D3) — exposed for the verdict tests.
    #[cfg(test)]
    pub(crate) fn collection_errors_all_poisoned(&self) -> bool {
        self.poison_compensated_error_count == self.errors.len()
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
        let graph = TypeGraph::build_adt_only(&adt_types);
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
