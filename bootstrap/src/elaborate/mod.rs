//! Elaborator: Surface AST to Core Terms
//!
//! The elaborator bridges the gap between the human-friendly surface syntax
//! and the mathematically-precise Core calculus that `tungsten_core` can process.
//!
//! ## Responsibilities
//!
//! 1. **Name resolution**: Resolve identifiers to definitions
//! 2. **Type inference**: Infer omitted type annotations (bidirectional)
//! 3. **Desugaring**: Convert surface constructs to core primitives
//! 4. **Validation**: Reject Phase 1 unsupported features with helpful errors
//! 5. **Core term construction**: Build `tungsten_core::Term` values
//!
//! ## Architecture
//!
//! ```text
//! Surface AST
//!      │
//!      ▼
//! ┌─────────────────────────────────────┐
//! │         NAME RESOLUTION             │
//! │  • Build symbol table from items    │
//! │  • Resolve identifiers              │
//! └──────────────────┬──────────────────┘
//!                    │
//!                    ▼
//! ┌─────────────────────────────────────┐
//! │    ELABORATION + TYPE INFERENCE     │
//! │  • Bidirectional type checking      │
//! │  • Infer omitted annotations        │

// Submodules
//! │  • Desugar surface constructs       │
//! │  • Build Core terms                 │
//! └──────────────────┬──────────────────┘
//!                    │
//!                    ▼
//!            Core Terms + Diagnostics
//! ```

mod env;
mod error;
mod exprs;
mod items;
mod types;

#[cfg(test)]
mod tests;

pub use env::{
    Constructor, ConstructorInfo, Env, ImportInfo, LocalBinding, ModuleContents, ModulePath,
    PathResolutionError, ResolvedValue, TypeDef, TypeDefKind, ValueDef,
};
pub use error::{ElabError, ElabErrorKind, ExpectedContext, ExpectedReason};

use std::collections::HashSet;

use crate::ast::{Item, SourceFile};
use crate::span::Span;
use serde::{Deserialize, Serialize};
use tungsten_core::{Context, Term, Type};

/// A fully elaborated definition ready for the Core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreDef {
    /// The name of this definition
    pub name: String,
    /// The type of this definition
    pub ty: Type,
    /// The term (value/proof) of this definition
    pub term: Term,
    /// Source span for error reporting
    pub span: Span,
}

/// Result of elaboration including warnings.
#[derive(Debug, Serialize, Deserialize)]
pub struct ElabOutput {
    /// The elaborated definitions (empty if there were errors)
    pub defs: Vec<CoreDef>,
    /// Non-fatal warnings encountered during elaboration
    pub warnings: Vec<ElabError>,
    /// Record type definitions: name -> fields.
    /// Used by codegen to expand `TyVar("RecordName")` to structural product types.
    pub record_types: std::collections::HashMap<String, Vec<(String, Type)>>,
    /// ADT type definitions: name -> (params, constructors).
    /// Used by codegen to expand `Type::App("Name", args)` to sum/mu types.
    pub adt_types: std::collections::HashMap<String, (Vec<String>, Vec<env::Constructor>)>,
}

/// Result type for elaboration
pub type ElabResult<T> = Result<T, ElabError>;

/// Elaborate a parsed source file to Core definitions.
///
/// This is the main entry point for elaboration. It:
/// 1. Collects all top-level definitions (first pass)
/// 2. Elaborates each definition to Core terms (second pass)
/// 3. Returns the elaborated definitions or accumulated errors
///
/// # Example
///
/// ```ignore
/// use tungsten_bootstrap::{parse, elaborate};
/// use tungsten_core::Context;
///
/// let source = "fn id(x: Nat) -> Nat { x }";
/// let (ast, parse_errors) = parse(source);
/// assert!(parse_errors.is_empty());
///
/// let mut ctx = Context::new();
/// match elaborate(&ast, &mut ctx) {
///     Ok(defs) => println!("Elaborated {} definitions", defs.len()),
///     Err(errors) => {
///         for e in errors {
///             eprintln!("Error: {}", e);
///         }
///     }
/// }
/// ```
pub fn elaborate(
    file: &SourceFile,
    core_ctx: &mut Context,
) -> Result<Vec<CoreDef>, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);
    elaborator.elaborate_file(file)
}

/// Elaborate a parsed source file to Core definitions, also returning warnings.
///
/// This is like `elaborate` but also returns warnings for non-fatal issues like
/// unreachable match arms. Warnings do not prevent compilation from succeeding.
pub fn elaborate_with_warnings(
    file: &SourceFile,
    core_ctx: &mut Context,
) -> Result<ElabOutput, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);
    match elaborator.elaborate_file(file) {
        Ok(defs) => Ok(ElabOutput {
            defs,
            warnings: std::mem::take(&mut elaborator.warnings),
            record_types: elaborator.get_record_types(),
            adt_types: elaborator.get_adt_types(),
        }),
        Err(errors) => Err(errors),
    }
}

/// Run only the collection pass (first pass of elaboration).
///
/// This collects all type and value definitions into the environment,
/// which can then be used to compute a types_hash for IR caching.
pub fn collect_definitions<'a>(
    file: &SourceFile,
    core_ctx: &'a mut Context,
) -> Result<CollectedElaborator<'a>, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);
    elaborator.run_collection_pass(file)?;
    Ok(CollectedElaborator {
        elaborator,
        file: file.clone(),
    })
}

/// Run the collection pass with module information.
///
/// Like `collect_definitions`, but populates the environment with module
/// information for qualified path resolution.
pub fn collect_definitions_with_modules<'a>(
    file: &SourceFile,
    core_ctx: &'a mut Context,
    modules: std::collections::HashMap<ModulePath, ModuleContents>,
    item_modules: std::collections::HashMap<String, ModulePath>,
    module_visibility: std::collections::HashMap<
        ModulePath,
        (crate::ast::Visibility, Option<ModulePath>),
    >,
    use_statement_modules: std::collections::HashMap<(std::path::PathBuf, u32), ModulePath>,
    use_statement_by_span: std::collections::HashMap<(u32, u32), ModulePath>,
    item_index_to_file: Vec<std::path::PathBuf>,
    module_files: std::collections::HashMap<ModulePath, std::path::PathBuf>,
    file_to_module: std::collections::HashMap<std::path::PathBuf, ModulePath>,
) -> Result<CollectedElaborator<'a>, Vec<ElabError>> {
    let mut elaborator = Elaborator::new(core_ctx);

    // Populate module registry
    elaborator.env.populate_module_info(
        modules,
        item_modules,
        module_visibility,
        use_statement_modules,
        use_statement_by_span,
        item_index_to_file,
        module_files,
        file_to_module,
    );

    elaborator.run_collection_pass(file)?;
    Ok(CollectedElaborator {
        elaborator,
        file: file.clone(),
    })
}

/// Result of running the collection pass.
///
/// This represents an elaborator that has completed the collection pass
/// and is ready to either:
/// - Compute a types hash for cache lookup
/// - Continue to the elaboration pass if cache miss
pub struct CollectedElaborator<'a> {
    elaborator: Elaborator<'a>,
    file: SourceFile,
}

impl<'a> CollectedElaborator<'a> {
    /// Get the collected types for computing a types hash.
    pub fn types_for_hash(&self) -> Vec<(String, TypeDef)> {
        self.elaborator.env.export_types_for_hash()
    }

    /// Get the collected value signatures for computing a types hash.
    pub fn value_signatures_for_hash(&self) -> Vec<(String, Type)> {
        self.elaborator.env.export_value_signatures_for_hash()
    }

    /// Continue to the elaboration pass after a cache miss.
    ///
    /// This consumes the CollectedElaborator and produces the final CoreDefs.
    pub fn elaborate(mut self) -> Result<ElabOutput, Vec<ElabError>> {
        // Pass 2: Elaborate each definition
        let mut defs = Vec::new();
        for item in &self.file.items {
            match self.elaborator.elaborate_item(item) {
                Ok(Some(def)) => defs.push(def),
                Ok(None) => {} // Type definitions don't produce CoreDefs
                Err(e) => self.elaborator.record_error(e), // Use record_error to attach file path
            }
        }

        if self.elaborator.errors.is_empty() {
            Ok(ElabOutput {
                defs,
                warnings: std::mem::take(&mut self.elaborator.warnings),
                record_types: self.elaborator.get_record_types(),
                adt_types: self.elaborator.get_adt_types(),
            })
        } else {
            Err(std::mem::take(&mut self.elaborator.errors))
        }
    }
}

/// Legacy result type for backwards compatibility.
#[derive(Debug)]
pub struct CollectionResult {
    /// All type definitions collected.
    pub types: Vec<(String, TypeDef)>,
    /// All value signatures collected.
    pub values: Vec<(String, Type)>,
}

use crate::config::MAX_CONTEXT_DEPTH;

/// The elaborator state machine.
///
/// Holds all context needed during elaboration:
/// - Name resolution environment
/// - Reference to Core context (for validation)
/// - Accumulated errors
/// - Current de Bruijn depth
/// - Context stack for "expected because..." messages
pub struct Elaborator<'a> {
    /// Name resolution environment
    env: Env,
    /// Reference to the Core context (for type validation)
    #[allow(dead_code)]
    core_ctx: &'a mut Context,
    /// Accumulated errors (we try to continue after errors)
    errors: Vec<ElabError>,
    /// Accumulated warnings (non-fatal diagnostics)
    warnings: Vec<ElabError>,
    /// Current de Bruijn depth for local variables
    depth: usize,
    /// Stack of type expectation contexts for error messages
    context_stack: Vec<ExpectedContext>,
    /// Counter for generating unique names
    name_counter: usize,
    /// Current module path (for visibility checking)
    current_module: ModulePath,
}

impl<'a> Elaborator<'a> {
    /// Create a new elaborator with an empty environment.
    pub fn new(core_ctx: &'a mut Context) -> Self {
        Self {
            env: Env::new(),
            core_ctx,
            errors: Vec::new(),
            warnings: Vec::new(),
            depth: 0,
            context_stack: Vec::new(),
            name_counter: 0,
            current_module: ModulePath::root(),
        }
    }

    /// Generate a fresh unique variable name with the given prefix.
    pub fn fresh_var(&mut self, prefix: &str) -> String {
        let name = format!("__{}{}", prefix, self.name_counter);
        self.name_counter += 1;
        name
    }

    /// Push a type expectation context onto the stack.
    pub fn push_context(&mut self, context: ExpectedContext) {
        if self.context_stack.len() < MAX_CONTEXT_DEPTH {
            self.context_stack.push(context);
        }
    }

    /// Pop the top context from the stack.
    pub fn pop_context(&mut self) {
        self.context_stack.pop();
    }

    /// Get the current type expectation context, if any.
    pub fn current_context(&self) -> Option<&ExpectedContext> {
        self.context_stack.last()
    }

    /// Get the current module path.
    pub fn get_current_module(&self) -> &ModulePath {
        &self.current_module
    }

    /// Get the file path for the current module (for error reporting).
    pub fn get_current_file(&self) -> Option<std::path::PathBuf> {
        self.env.get_module_file(&self.current_module).cloned()
    }

    /// Add file path to an error based on the current module.
    pub fn error_with_file(&self, mut error: ElabError) -> ElabError {
        if let Some(file_path) = self.get_current_file() {
            error = error.with_file_path(file_path);
        }
        error
    }

    /// Record an error with file path attached.
    pub fn record_error(&mut self, error: ElabError) {
        self.errors.push(self.error_with_file(error));
    }

    /// Record a warning with file path attached.
    pub fn record_warning(&mut self, warning: ElabError) {
        self.warnings.push(self.error_with_file(warning));
    }

    /// Set the current module path (for entering nested modules).
    pub fn set_current_module(&mut self, module: ModulePath) {
        self.current_module = module;
    }

    /// Extract record type definitions for codegen.
    ///
    /// Returns a map from record name to its fields.
    /// This is needed because records are kept as nominal types during elaboration
    /// but codegen needs to expand them to structural product types.
    pub fn get_record_types(&self) -> std::collections::HashMap<String, Vec<(String, Type)>> {
        let mut records = std::collections::HashMap::new();
        for (name, type_def) in &self.env.types {
            if let TypeDefKind::Record(fields) = &type_def.kind {
                records.insert(name.clone(), fields.clone());
            }
        }
        records
    }

    /// Extract ADT type definitions for codegen.
    ///
    /// Returns a map from ADT name to (params, constructors).
    /// This is needed to expand `Type::App("Name", args)` to sum/mu types in codegen.
    pub fn get_adt_types(
        &self,
    ) -> std::collections::HashMap<String, (Vec<String>, Vec<env::Constructor>)> {
        let mut adts = std::collections::HashMap::new();
        for (name, type_def) in &self.env.types {
            if let TypeDefKind::ADT(constructors) = &type_def.kind {
                adts.insert(
                    name.clone(),
                    (type_def.params.clone(), constructors.clone()),
                );
            }
        }
        adts
    }

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

        // Phase 1c: Fully collect TYPE definitions (elaborate bodies)
        // Now that imports are available, type bodies can reference imported types
        for item in &file.items {
            if matches!(item, Item::TypeDef(_) | Item::TypeAlias(_)) {
                if let Err(e) = self.collect_item(item) {
                    self.record_error(e);
                }
            }
        }

        // Phase 1d: Resolve deferred type references
        // During Phase 1c, some types may have been elaborated before the types they
        // reference (due to AST order). Those references were stored as TyVars.
        // Now that all types are fully elaborated, resolve those TyVars.
        self.resolve_deferred_type_references();

        // Phase 1e: Cache encoded types for non-parameterized types
        // This enables reverse lookup from Core types to user-defined names
        // for cleaner error messages.
        self.cache_type_encodings();

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
                if let Some(existing) = self.env.lookup_type(&type_def.name.name) {
                    if !matches!(existing.kind, env::TypeDefKind::Stub) {
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
                if let Some(existing) = self.env.lookup_type(&alias.name.name) {
                    if !matches!(existing.kind, env::TypeDefKind::Stub) {
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

    /// Resolve deferred type references (Phase 1d).
    ///
    /// During Phase 1c, types may reference other types that haven't been elaborated yet
    /// (due to AST order). Those references are stored as TyVars. Now that all types
    /// are elaborated, we resolve those TyVars to their actual type encodings.
    fn resolve_deferred_type_references(&mut self) {
        // Collect type names to process (can't iterate and mutate at the same time)
        let type_names: Vec<String> = self.env.iter_types().map(|(k, _)| k.clone()).collect();

        for name in type_names {
            // Get the current type definition
            let type_def = match self.env.lookup_type(&name) {
                Some(td) => td.clone(),
                None => continue,
            };

            // Skip types that are still stubs (shouldn't happen after Phase 1c)
            if matches!(type_def.kind, TypeDefKind::Stub) {
                continue;
            }

            // Resolve TyVars in the type's body
            let resolved_kind = match &type_def.kind {
                TypeDefKind::Alias(ty) => {
                    let resolved = self.resolve_tyvars_in_type(ty, &name);
                    TypeDefKind::Alias(resolved)
                }
                TypeDefKind::Record(fields) => {
                    let resolved_fields: Vec<_> = fields
                        .iter()
                        .map(|(field_name, field_ty)| {
                            (
                                field_name.clone(),
                                self.resolve_tyvars_in_type(field_ty, &name),
                            )
                        })
                        .collect();
                    TypeDefKind::Record(resolved_fields)
                }
                TypeDefKind::ADT(ctors) => {
                    let resolved_ctors: Vec<_> = ctors
                        .iter()
                        .map(|ctor| Constructor {
                            name: ctor.name.clone(),
                            fields: ctor
                                .fields
                                .iter()
                                .map(|f| self.resolve_tyvars_in_type(f, &name))
                                .collect(),
                            index: ctor.index,
                            span: ctor.span,
                        })
                        .collect();
                    TypeDefKind::ADT(resolved_ctors)
                }
                TypeDefKind::Stub => continue,
            };

            // Update the type definition with resolved types
            let mut updated = type_def;
            updated.kind = resolved_kind;
            // Clear any cached encoding since we've modified the type
            updated.encoded_type = None;
            self.env.types.insert(name, updated);
        }
    }

    /// Resolve TyVars that refer to defined types.
    ///
    /// This is used in Phase 1d to resolve cross-module type references
    /// that were deferred because the target type wasn't elaborated yet.
    fn resolve_tyvars_in_type(&mut self, ty: &Type, skip_name: &str) -> Type {
        let mut encoding_stack = HashSet::new();
        encoding_stack.insert(skip_name.to_string());
        self.resolve_tyvars_in_type_impl(ty, &mut encoding_stack)
    }

    /// Internal implementation of resolve_tyvars_in_type with cycle detection.
    fn resolve_tyvars_in_type_impl(
        &mut self,
        ty: &Type,
        encoding_stack: &mut HashSet<String>,
    ) -> Type {
        match ty {
            Type::TyVar(name) if !encoding_stack.contains(name) => {
                // Check if this TyVar refers to a now-defined type
                if let Some(type_def) = self.env.lookup_type(name).cloned() {
                    // Only resolve non-parameterized, non-stub types
                    if type_def.params.is_empty() && !matches!(type_def.kind, TypeDefKind::Stub) {
                        // Add to stack before resolving to detect cycles
                        encoding_stack.insert(name.clone());
                        let result = match &type_def.kind {
                            TypeDefKind::Alias(alias_ty) => {
                                self.resolve_tyvars_in_type_impl(alias_ty, encoding_stack)
                            }
                            TypeDefKind::Record(_) => {
                                // Keep record as nominal type - encoding happens at codegen
                                ty.clone()
                            }
                            TypeDefKind::ADT(_) => {
                                // For ADTs, encode them with shared encoding_stack
                                if let Ok(encoded) =
                                    self.encode_adt_type_impl(name, &[], encoding_stack)
                                {
                                    encoded
                                } else {
                                    ty.clone()
                                }
                            }
                            TypeDefKind::Stub => ty.clone(),
                        };
                        encoding_stack.remove(name);
                        result
                    } else {
                        ty.clone()
                    }
                } else {
                    ty.clone()
                }
            }
            Type::TyVar(_) => ty.clone(), // In encoding stack or bound variable
            Type::Arrow(a, b) => Type::arrow(
                self.resolve_tyvars_in_type_impl(a, encoding_stack),
                self.resolve_tyvars_in_type_impl(b, encoding_stack),
            ),
            Type::Product(a, b) => Type::product(
                self.resolve_tyvars_in_type_impl(a, encoding_stack),
                self.resolve_tyvars_in_type_impl(b, encoding_stack),
            ),
            Type::Sum(a, b) => Type::sum(
                self.resolve_tyvars_in_type_impl(a, encoding_stack),
                self.resolve_tyvars_in_type_impl(b, encoding_stack),
            ),
            Type::Forall(v, body) => Type::forall(
                v.clone(),
                self.resolve_tyvars_in_type_impl(body, encoding_stack),
            ),
            Type::Mu(v, body) => Type::mu(
                v.clone(),
                self.resolve_tyvars_in_type_impl(body, encoding_stack),
            ),
            Type::Eq(ty_arg, a, b) => Type::eq(
                self.resolve_tyvars_in_type_impl(ty_arg, encoding_stack),
                (**a).clone(),
                (**b).clone(),
            ),
            Type::Nat | Type::Bool | Type::Unit | Type::Void | Type::Prop | Type::String => {
                ty.clone()
            }
            Type::Ptr(inner) => Type::ptr(self.resolve_tyvars_in_type_impl(inner, encoding_stack)),
            Type::Ref(inner) => {
                Type::ref_ty(self.resolve_tyvars_in_type_impl(inner, encoding_stack))
            }
            // Deferred type application: resolve and expand
            Type::App(name, args) if !encoding_stack.contains(name) => {
                // Resolve arguments first
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_tyvars_in_type_impl(a, encoding_stack))
                    .collect();

                // Add to stack before expanding to detect cycles
                encoding_stack.insert(name.clone());

                // Try to expand the type application
                let result = if let Some(type_def) = self.env.lookup_type(name).cloned() {
                    if !matches!(type_def.kind, TypeDefKind::Stub) {
                        match &type_def.kind {
                            TypeDefKind::ADT(_) => {
                                // Encode the ADT with the resolved type arguments
                                if let Ok(encoded) =
                                    self.encode_adt_type_impl(name, &resolved_args, encoding_stack)
                                {
                                    encoded
                                } else {
                                    Type::app(name.clone(), resolved_args)
                                }
                            }
                            TypeDefKind::Alias(alias_ty) => {
                                // Substitute type params in the alias
                                let mut result = alias_ty.clone();
                                for (param, arg) in type_def.params.iter().zip(resolved_args.iter())
                                {
                                    result = result.substitute(param, arg);
                                }
                                self.resolve_tyvars_in_type_impl(&result, encoding_stack)
                            }
                            TypeDefKind::Record(_) => {
                                // Keep record as nominal type - encoding happens at codegen
                                // Return as App with resolved type arguments
                                Type::app(name.clone(), resolved_args)
                            }
                            TypeDefKind::Stub => Type::app(name.clone(), resolved_args),
                        }
                    } else {
                        Type::app(name.clone(), resolved_args)
                    }
                } else {
                    // Couldn't expand - keep as App with resolved args
                    Type::app(name.clone(), resolved_args)
                };

                encoding_stack.remove(name);
                result
            }
            Type::App(name, args) => {
                // Type is in encoding stack (cycle detected) - just resolve args
                let resolved_args: Vec<Type> = args
                    .iter()
                    .map(|a| self.resolve_tyvars_in_type_impl(a, encoding_stack))
                    .collect();
                Type::app(name.clone(), resolved_args)
            }
            // Flat ADT (ADR 2.2.26) - recursively resolve type vars in variants
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args
                    .iter()
                    .map(|t| self.resolve_tyvars_in_type_impl(t, encoding_stack))
                    .collect(),
                variants
                    .iter()
                    .map(|(ctor, payload)| {
                        (
                            ctor.clone(),
                            self.resolve_tyvars_in_type_impl(payload, encoding_stack),
                        )
                    })
                    .collect(),
            ),
        }
    }

    /// Cache encoded types for type name reverse lookup (Phase 1e).
    ///
    /// This enables reverse lookup from Core types to user-defined type names
    /// for cleaner error messages.
    ///
    /// - Non-parameterized types (like `Color`) are registered for exact match.
    /// - Parameterized types (like `Option<T>`) are registered as patterns for
    ///   structural matching.
    fn cache_type_encodings(&mut self) {
        use crate::driver::{register_type_name, register_type_pattern};

        // Collect all non-stub type names
        let type_names: Vec<String> = self
            .env
            .iter_types()
            .filter(|(_, def)| !matches!(def.kind, TypeDefKind::Stub))
            .map(|(name, _)| name.clone())
            .collect();

        for name in type_names {
            let type_def = match self.env.lookup_type(&name) {
                Some(td) => td.clone(),
                None => continue,
            };

            if type_def.params.is_empty() {
                // Non-parameterized type: register for exact match
                if type_def.encoded_type.is_some() {
                    continue; // Already cached
                }

                let encoded = match &type_def.kind {
                    TypeDefKind::Alias(ty) => Some(ty.clone()),
                    TypeDefKind::Record(fields) => Some(self.encode_record_type(fields)),
                    TypeDefKind::ADT(_) => self.encode_adt_type(&name, &[]).ok(),
                    TypeDefKind::Stub => None,
                };

                if let Some(encoded) = encoded.clone() {
                    if let Some(def) = self.env.types.get_mut(&name) {
                        def.encoded_type = Some(encoded.clone());
                    }
                    register_type_name(encoded, name.clone());
                }
            } else {
                // Parameterized type: register as a pattern
                // Build a pattern with TyVar placeholders for each parameter
                let pattern = self.build_type_pattern(&name, &type_def);
                if let Some(pattern) = pattern {
                    register_type_pattern(pattern);
                }
            }
        }
    }

    /// Build a type pattern for a parameterized type.
    ///
    /// For `Option<T>`, returns a pattern `Unit + TyVar("T")`.
    /// For `List<T>`, returns a pattern `μα_List. Unit + (TyVar("T") × TyVar("α_List"))`.
    fn build_type_pattern(
        &mut self,
        name: &str,
        type_def: &TypeDef,
    ) -> Option<crate::driver::TypePattern> {
        use crate::driver::TypePattern;

        // Create type args as TyVars for the pattern
        let type_args: Vec<Type> = type_def
            .params
            .iter()
            .map(|p| Type::TyVar(p.clone()))
            .collect();

        // Encode the type with TyVar placeholders
        let pattern = match &type_def.kind {
            TypeDefKind::ADT(_) => self.encode_adt_type(name, &type_args).ok()?,
            TypeDefKind::Record(fields) => {
                // For records with type params, substitute in the pattern
                self.encode_record_type_with_args(fields, &type_def.params, &type_args)
            }
            TypeDefKind::Alias(ty) => {
                // Substitute type params in the alias body
                self.substitute_type_params(ty, &type_def.params, &type_args)
            }
            TypeDefKind::Stub => return None,
        };

        // Check if this is a recursive type (has a μ-binder)
        let mu_var = match &pattern {
            Type::Mu(v, _) => Some(v.clone()),
            _ => None,
        };

        Some(TypePattern {
            name: name.to_string(),
            params: type_def.params.clone(),
            pattern,
            mu_var,
        })
    }

    /// Encode a record type with explicit type arguments substituted.
    fn encode_record_type_with_args(
        &self,
        fields: &[(String, Type)],
        params: &[String],
        args: &[Type],
    ) -> Type {
        // Substitute type params in each field
        let substituted_fields: Vec<(String, Type)> = fields
            .iter()
            .map(|(name, ty)| (name.clone(), self.substitute_type_params(ty, params, args)))
            .collect();
        self.encode_record_type(&substituted_fields)
    }

    /// Substitute type parameters in a type.
    fn substitute_type_params(&self, ty: &Type, params: &[String], args: &[Type]) -> Type {
        match ty {
            Type::TyVar(v) => {
                // Check if this is a type parameter to substitute
                if let Some(idx) = params.iter().position(|p| p == v) {
                    args.get(idx).cloned().unwrap_or_else(|| ty.clone())
                } else {
                    ty.clone()
                }
            }
            Type::Arrow(a, b) => Type::Arrow(
                Box::new(self.substitute_type_params(a, params, args)),
                Box::new(self.substitute_type_params(b, params, args)),
            ),
            Type::Product(a, b) => Type::Product(
                Box::new(self.substitute_type_params(a, params, args)),
                Box::new(self.substitute_type_params(b, params, args)),
            ),
            Type::Sum(a, b) => Type::Sum(
                Box::new(self.substitute_type_params(a, params, args)),
                Box::new(self.substitute_type_params(b, params, args)),
            ),
            Type::Forall(v, body) => Type::Forall(
                v.clone(),
                Box::new(self.substitute_type_params(body, params, args)),
            ),
            Type::Mu(v, body) => Type::Mu(
                v.clone(),
                Box::new(self.substitute_type_params(body, params, args)),
            ),
            Type::Ptr(inner) => {
                Type::Ptr(Box::new(self.substitute_type_params(inner, params, args)))
            }
            Type::Ref(inner) => {
                Type::Ref(Box::new(self.substitute_type_params(inner, params, args)))
            }
            Type::Eq(inner_ty, t1, t2) => Type::Eq(
                Box::new(self.substitute_type_params(inner_ty, params, args)),
                t1.clone(),
                t2.clone(),
            ),
            Type::App(name, type_args) => Type::App(
                name.clone(),
                type_args
                    .iter()
                    .map(|a| self.substitute_type_params(a, params, args))
                    .collect(),
            ),
            // Base types pass through unchanged
            _ => ty.clone(),
        }
    }

    /// Record an error and continue (for error recovery).
    #[allow(dead_code)]
    fn error(&mut self, span: Span, kind: ElabErrorKind) -> ElabError {
        let err = ElabError::new(span, kind);
        err
    }

    /// Record an error with a help message.
    #[allow(dead_code)]
    fn error_with_help(&mut self, span: Span, kind: ElabErrorKind, help: &str) -> ElabError {
        let mut err = ElabError::new(span, kind);
        err.help = Some(help.to_string());
        err
    }

    /// Record a warning (non-fatal diagnostic).
    pub fn warn(&mut self, warning: ElabError) {
        self.record_warning(warning);
    }
}
