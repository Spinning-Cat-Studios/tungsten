//! Type Lowering
//!
//! Maps Tungsten Core types to LLVM IR types.
//!
//! # Type Mapping
//!
//! | Tungsten Type | LLVM Type                          |
//! |---------------|------------------------------------|
//! | Bool          | i1                                 |
//! | Nat           | i64                                |
//! | Unit          | {} (empty struct)                  |
//! | Void          | void (never constructed)           |
//! | String        | { i8*, i64 } (ptr + length)        |
//! | τ₁ → τ₂       | { fn(env*, args..)->ret, env* }    |
//! | τ₁ × τ₂       | { τ₁_llvm, τ₂_llvm }               |
//! | τ₁ + τ₂       | { i32 tag, largest(τ₁, τ₂) }       |
//! | ∀α. τ         | (type erased at runtime)           |
//! | Eq τ t₁ t₂    | {} (proof irrelevant)              |
//! | Prop          | {} (proof irrelevant)              |
//! | μα. τ         | i8* (opaque pointer)               |

mod analysis;
mod encoding;
mod lowering;
mod setup;

use inkwell::context::Context;
use inkwell::targets::TargetData;
use inkwell::types::StructType;
use std::collections::{HashMap, HashSet};
use tungsten_core::types::Type;

/// Strip the `@` prefix from named type `TyVars` (ADR 13.4.26c §2).
///
/// Named types (records, stubs) use `@`-prefix in `TyVar` names
/// (e.g., `TyVar("@Token")`) to distinguish them from genuine type
/// variables. Lookups in `record_types`/`adt_types` use the unprefixed name.
pub(crate) fn strip_named_prefix(name: &str) -> &str {
    name.strip_prefix('@').unwrap_or(name)
}

/// A simplified constructor for codegen purposes.
#[derive(Debug, Clone)]
pub struct CodegenConstructor {
    /// Constructor name (e.g., "Some", "None")
    pub name: String,
    /// Field types (positional)
    pub fields: Vec<Type>,
    /// Index of this constructor in the ADT  
    pub index: usize,
}

/// ADT definition for codegen: params + constructors.
pub type AdtDef = (Vec<String>, Vec<CodegenConstructor>);

/// Manages the mapping from Tungsten types to LLVM types.
pub struct TypeLowering<'ctx> {
    pub(crate) context: &'ctx Context,
    /// Cache of struct types for ADT types (name -> LLVM struct type)
    pub(crate) adt_type_cache: HashMap<String, inkwell::types::BasicTypeEnum<'ctx>>,
    /// Record types: name -> fields.
    /// Used to expand `TyVar("RecordName")` to the structural product type.
    pub(crate) record_types: HashMap<String, Vec<(String, Type)>>,
    /// ADT types: name -> (params, constructors).
    /// Used to expand `Type::App("Name", args)` to sum/mu types.
    pub(crate) adt_types: HashMap<String, AdtDef>,
    /// Type variable substitutions for monomorphization.
    /// Used to lower type variables to their concrete types when compiling
    /// type-applied expressions.
    pub(crate) type_subst: HashMap<String, Type>,
    /// Target data for accurate type size calculation with alignment.
    /// When present, uses LLVM's `get_store_size` for precise sizes.
    pub(crate) target_data: Option<TargetData>,
    /// Count of `TyVar` fallthrough occurrences in `lower_type`.
    /// Non-zero means the elaborator is leaking unresolved type variables to codegen.
    /// TRANSITIONAL: expected to reach 0 after W2.1 (ADR 11.4.26c).
    pub(crate) tyvar_fallthrough_count: usize,
    /// Name of the definition currently being compiled.
    /// Used for diagnostic context in `TyVar` fallthrough warnings.
    pub(crate) current_def_name: Option<String>,
    /// When true, capture and display backtraces on `TyVar` fallthrough.
    /// Activated by --codegen-backtrace CLI flag (ADR 13.4.26b W3.1 Tool 4).
    pub(crate) codegen_backtrace: bool,
    /// Current recursion depth in `lower_type`.
    /// Used to detect infinite recursion during type lowering.
    pub(crate) lower_type_depth: usize,
    /// Set of ADT names currently being lowered via `lower_app`.
    /// Used to break cycles: if we re-enter `lower_app` for a name already
    /// in this set, we return an opaque pointer instead of recursing.
    pub(crate) lowering_in_progress: HashSet<String>,
    /// Cached set of known concrete type names (ADT ∪ record keys).
    /// Used by `Type::has_mono_blocking_tyvar` to distinguish concrete
    /// cross-module type references from abstract type parameters.
    /// Populated by `register_adt_types` / `register_record_types`.
    pub(crate) concrete_type_names: HashSet<String>,
}

impl<'ctx> TypeLowering<'ctx> {
    /// Create a new type lowering context.
    #[must_use]
    pub fn new(context: &'ctx Context) -> Self {
        Self {
            context,
            adt_type_cache: HashMap::new(),
            record_types: HashMap::new(),
            adt_types: HashMap::new(),
            type_subst: HashMap::new(),
            target_data: None,
            tyvar_fallthrough_count: 0,
            current_def_name: None,
            codegen_backtrace: false,
            lower_type_depth: 0,
            lowering_in_progress: HashSet::new(),
            concrete_type_names: HashSet::new(),
        }
    }

    /// Set target data for accurate type size calculation.
    /// When set, uses LLVM's `get_store_size` for precise sizes including alignment.
    pub fn set_target_data(&mut self, target_data: TargetData) {
        self.target_data = Some(target_data);
    }

    /// Set the name of the definition currently being compiled.
    /// Used for diagnostic context in `TyVar` fallthrough warnings.
    pub fn set_current_def_name(&mut self, name: &str) {
        self.current_def_name = Some(name.to_string());
    }

    /// Get the name of the definition currently being compiled.
    #[must_use]
    pub fn current_def_name(&self) -> Option<&str> {
        self.current_def_name.as_deref()
    }

    /// Enable codegen backtrace capture on `TyVar` fallthrough.
    pub fn set_codegen_backtrace(&mut self, enabled: bool) {
        self.codegen_backtrace = enabled;
    }

    /// Get the LLVM context.
    #[must_use]
    pub fn context(&self) -> &'ctx Context {
        self.context
    }

    /// Create the string type.
    #[must_use]
    pub fn string_type(&self) -> StructType<'ctx> {
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let len_type = self.context.i64_type();
        self.context
            .struct_type(&[ptr_type.into(), len_type.into()], false)
    }

    /// Get the type used for sum type tags.
    #[must_use]
    pub fn tag_type(&self) -> inkwell::types::IntType<'ctx> {
        self.context.i8_type()
    }
}

#[cfg(test)]
mod tests;
